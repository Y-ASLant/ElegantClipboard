#[cfg(test)]
mod tests;

use gpui_kit::{ImageCacheError, ImageSource, RenderImage};
use image::{DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

const MAX_CONCURRENT: usize = 2;
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
const FILE_RECHECK: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ThumbnailKey {
    pub path: PathBuf,
    pub content_hash: String,
    pub external: bool,
    pub size: (u32, u32),
}

impl ThumbnailKey {
    pub fn new(
        path: PathBuf,
        content_hash: String,
        external: bool,
        logical_size: (f32, f32),
        scale: f32,
    ) -> Self {
        let dimension = |value: f32| (value * scale).ceil().clamp(1., 2048.) as u32;
        Self {
            path,
            content_hash,
            external,
            size: (dimension(logical_size.0), dimension(logical_size.1)),
        }
    }
}

pub(super) enum Thumbnail {
    Ready(Arc<RenderImage>),
    Original,
    Failed(ImageCacheError),
}

impl Thumbnail {
    fn source(&self, key: &ThumbnailKey) -> ImageSource {
        match self {
            Self::Ready(image) => image.clone().into(),
            Self::Original => key.path.clone().into(),
            Self::Failed(error) => {
                let error = error.clone();
                ImageSource::Custom(Arc::new(move |_, _| Some(Err(error.clone()))))
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct FileVersion {
    len: u64,
    modified: SystemTime,
}

struct Entry {
    token: u64,
    result: Option<Thumbnail>,
    version: Option<FileVersion>,
    checked: Instant,
}

pub(super) struct ThumbnailJob {
    pub token: u64,
    key: ThumbnailKey,
    previous: Option<FileVersion>,
}

pub(super) struct ThumbnailResult {
    version: Option<FileVersion>,
    image: Option<Thumbnail>,
}

#[derive(Default)]
pub(super) struct ThumbnailCache {
    wanted: Vec<ThumbnailKey>,
    entries: HashMap<ThumbnailKey, Entry>,
    running: HashMap<u64, ThumbnailKey>,
    next_token: u64,
    // A drag snapshot can outlive its row. Delay atlas removal until it releases its Arc.
    retired: Vec<Arc<RenderImage>>,
}

impl ThumbnailCache {
    pub fn retain(&mut self, wanted: Vec<ThumbnailKey>) {
        self.entries.retain(|key, entry| {
            if wanted.contains(key) {
                return true;
            }
            if let Some(Thumbnail::Ready(image)) = entry.result.take() {
                self.retired.push(image);
            }
            false
        });
        self.wanted = wanted;
    }

    pub fn source(&self, key: &ThumbnailKey) -> Option<ImageSource> {
        self.entries
            .get(key)
            .and_then(|entry| entry.result.as_ref())
            .map(|result| result.source(key))
    }

    pub fn next_job(&mut self) -> Option<ThumbnailJob> {
        if self.running.len() >= MAX_CONCURRENT {
            return None;
        }
        let key = self
            .wanted
            .iter()
            .find(|key| !self.entries.contains_key(*key))
            .or_else(|| {
                self.wanted.iter().find(|key| {
                    key.external
                        && self.entries.get(*key).is_some_and(|entry| {
                            !self.running.contains_key(&entry.token)
                                && entry.checked.elapsed() >= FILE_RECHECK
                        })
                })
            })?
            .clone();
        self.next_token += 1;
        let token = self.next_token;
        let entry = self.entries.entry(key.clone()).or_insert_with(|| Entry {
            token,
            result: None,
            version: None,
            checked: Instant::now(),
        });
        entry.token = token;
        self.running.insert(token, key.clone());
        Some(ThumbnailJob {
            token,
            key,
            previous: entry.version.clone(),
        })
    }

    pub fn finish(&mut self, token: u64, result: ThumbnailResult) -> Option<PathBuf> {
        let key = self.running.remove(&token)?;
        // Eviction and re-entry can queue another request for the very same key.
        let entry = self
            .entries
            .get_mut(&key)
            .filter(|entry| entry.token == token)?;
        entry.checked = Instant::now();
        entry.version = result.version;
        if let Some(image) = result.image {
            if let Some(Thumbnail::Ready(old)) = entry.result.replace(image) {
                self.retired.push(old);
            }
            return Some(key.path);
        }
        None
    }

    pub fn released_images(&mut self) -> Vec<Arc<RenderImage>> {
        let mut released = Vec::new();
        self.retired.retain(|image| {
            if Arc::strong_count(image) == 1 {
                released.push(image.clone());
                false
            } else {
                true
            }
        });
        released
    }

    pub fn drain_images(&mut self) -> Vec<Arc<RenderImage>> {
        self.retain(Vec::new());
        std::mem::take(&mut self.retired)
    }
}

impl ThumbnailJob {
    pub fn run(self) -> ThumbnailResult {
        // Metadata, format probing and decoding all share the same two background slots.
        let version = self
            .key
            .external
            .then(|| {
                let metadata = std::fs::metadata(&self.key.path)?;
                Ok::<_, std::io::Error>(FileVersion {
                    len: metadata.len(),
                    modified: metadata.modified()?,
                })
            })
            .transpose();
        let version = match version {
            Ok(version) => version,
            Err(error) => {
                return ThumbnailResult {
                    version: None,
                    image: Some(Thumbnail::Failed(error.into())),
                };
            }
        };
        if version.is_some() && version == self.previous {
            return ThumbnailResult {
                version,
                image: None,
            };
        }
        let image = match decode_thumbnail(&self.key) {
            Ok(Some(image)) => Thumbnail::Ready(image),
            Ok(None) => Thumbnail::Original,
            Err(error) => Thumbnail::Failed(error.into()),
        };
        let version = if matches!(image, Thumbnail::Failed(_)) {
            None
        } else {
            version
        };
        ThumbnailResult {
            version,
            image: Some(image),
        }
    }
}

fn decode_thumbnail(key: &ThumbnailKey) -> anyhow::Result<Option<Arc<RenderImage>>> {
    let mut reader = ImageReader::open(&key.path)?.with_guessed_format()?;
    if matches!(reader.format(), Some(ImageFormat::Gif | ImageFormat::WebP)) {
        // Preserve GPUI's multi-frame decoding even with a misleading file extension.
        return Ok(None);
    }
    if reader.format() == Some(ImageFormat::Png)
        && image::codecs::png::PngDecoder::new(std::io::BufReader::new(std::fs::File::open(
            &key.path,
        )?))?
        .is_apng()?
    {
        return Ok(None);
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    anyhow::ensure!(
        decoder.total_bytes() <= MAX_DECODE_BYTES,
        "Image exceeds thumbnail decode limit"
    );
    let orientation = decoder.orientation()?;
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    let (width, height) = key.size;
    let mut pixels = if decoded.width() > width || decoded.height() > height {
        decoded.thumbnail(width, height).into_rgba8()
    } else {
        decoded.into_rgba8()
    };
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Some(Arc::new(RenderImage::new(vec![Frame::new(pixels)]))))
}
