//! A single bounded renderer keeps image decoding off the history and UI threads.
use anyhow::{Context, Result, bail};
use clipboard_core::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
use std::{
    io::Read,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, OnceLock, Weak, mpsc},
};

type RenderResult = std::result::Result<Vec<u8>, String>;
type Queue = mpsc::SyncSender<Weak<DeferredDib>>;

#[derive(Debug, Default)]
struct State {
    queued: bool,
    result: Option<RenderResult>,
}

#[derive(Debug)]
pub(super) struct DeferredDib {
    path: String,
    state: Mutex<State>,
    ready: Condvar,
}

impl DeferredDib {
    pub(super) fn prepare(path: String) -> Arc<Self> {
        let dib = Arc::new(Self {
            path,
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
        });
        // Never wait for another image while preparing the inexpensive formats.
        let mut state = dib.state.lock().unwrap();
        if let Ok(queue) = renderer() {
            state.queued = queue.try_send(Arc::downgrade(&dib)).is_ok();
        }
        drop(state);
        dib
    }

    pub(super) fn with_bytes<T>(
        self: &Arc<Self>,
        consume: impl FnOnce(&[u8]) -> windows::core::Result<T>,
    ) -> windows::core::Result<T> {
        let mut state = self.state.lock().unwrap();
        if !state.queued && state.result.is_none() {
            state.queued = true;
            // GetData has a synchronous COM contract. A requesting consumer may
            // wait for the bounded renderer, but never runs image code itself.
            drop(state);
            let scheduled = renderer().and_then(|queue| {
                queue
                    .send(Arc::downgrade(self))
                    .map_err(|_| "图片拖放渲染线程已退出".to_owned())
            });
            state = self.state.lock().unwrap();
            if let Err(error) = scheduled {
                state.result = Some(Err(error));
                self.ready.notify_all();
            }
        }
        while state.result.is_none() {
            state = self.ready.wait(state).unwrap();
        }
        match state.result.as_ref().unwrap() {
            Ok(bytes) => consume(bytes),
            Err(error) => Err(windows::core::Error::new(
                windows::Win32::Foundation::E_FAIL,
                error,
            )),
        }
    }
}

fn renderer() -> std::result::Result<&'static Queue, String> {
    static QUEUE: OnceLock<std::result::Result<Queue, String>> = OnceLock::new();
    QUEUE
        .get_or_init(|| {
            // At most one active and one queued image. Weak queued references
            // allow abandoned drags to disappear without decoding their image.
            let (sender, receiver) = mpsc::sync_channel::<Weak<DeferredDib>>(1);
            std::thread::Builder::new()
                .name("drag-dib-renderer".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        let Some(job) = job.upgrade() else {
                            continue;
                        };
                        let result = catch_unwind(AssertUnwindSafe(|| render_dib(&job.path)))
                            .map_err(|_| "图片拖放渲染异常".to_owned())
                            .and_then(|result| result.map_err(|error| format!("{error:#}")));
                        job.state.lock().unwrap().result = Some(result);
                        job.ready.notify_all();
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(sender)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn render_dib(path: &str) -> Result<Vec<u8>> {
    let image = decode_image(path)?;
    let mut bitmap = std::io::Cursor::new(Vec::new());
    // A 24-bit BI_RGB DIB also supports older OLE consumers. The encoder
    // supplies the bitmap header and correctly padded bottom-up scanlines.
    image::DynamicImage::ImageRgb8(image.into_rgb8())
        .write_to(&mut bitmap, image::ImageFormat::Bmp)?;
    let mut dib = bitmap.into_inner();
    dib.drain(..14);
    Ok(dib)
}

fn decode_image(path: &str) -> Result<image::DynamicImage> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_IMAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_IMAGE_BYTES {
        bail!("拖放图片超过 50 MiB");
    }
    let reader = || image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format();
    let (width, height) = reader()?
        .into_dimensions()
        .context("无法读取拖放图片尺寸")?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        bail!("拖放图片尺寸无效或超过 2500 万像素");
    }
    let mut reader = reader()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 8);
    reader.limits(limits);
    reader.decode().context("无法读取拖放图片")
}
