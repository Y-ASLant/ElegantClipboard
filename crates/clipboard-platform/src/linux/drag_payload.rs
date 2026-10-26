use anyhow::{Context, Result, bail};
use clipboard_core::{History, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, database::ClipboardItem};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

type EncodedFormat = (&'static str, Arc<[u8]>);

#[derive(Clone, Debug)]
pub struct PreparedExternalDrag {
    pub(super) formats: Arc<Vec<EncodedFormat>>,
    files: Option<Arc<[PathBuf]>>,
}

impl PreparedExternalDrag {
    pub fn file_paths(&self) -> Option<&[PathBuf]> {
        self.files.as_deref()
    }
}

pub fn prepare_external_drag(
    item: &ClipboardItem,
    staged_dir: &Path,
) -> Result<PreparedExternalDrag> {
    let mut formats: Vec<EncodedFormat> = Vec::new();
    let mut files = None;
    match item.content_type.as_str() {
        "files" => {
            let paths = History::file_paths_for_copy(item, staged_dir)?;
            formats.push(("text/uri-list", file_uris(&paths)?.into()));
            files = Some(
                paths
                    .into_iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
                    .into(),
            );
        }
        "image" => {
            let path = item
                .image_path
                .as_deref()
                .context("Image path is missing")?;
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES as u64 {
                bail!("Drag image is invalid or exceeds 50 MiB");
            }
            formats.push(("text/uri-list", file_uris(&[path.to_owned()])?.into()));
            files = Some(vec![PathBuf::from(path)].into());
            let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
            reader.limits(limits);
            let image = reader.decode()?;
            if u64::from(image.width()) * u64::from(image.height()) > MAX_IMAGE_PIXELS {
                bail!("Drag image exceeds 25 million pixels");
            }
            let mut png = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png)?;
            formats.push(("image/png", png.into_inner().into()));
        }
        "text" | "url" | "html" | "rtf" => {
            if let Some(text) = item.text_content.as_deref().filter(|text| !text.is_empty()) {
                let bytes: Arc<[u8]> = text.as_bytes().into();
                formats.push(("text/plain;charset=utf-8", bytes.clone()));
                formats.push(("UTF8_STRING", bytes));
            }
            if let Some(html) = item.html_content.as_deref().filter(|text| !text.is_empty()) {
                formats.push(("text/html", html.as_bytes().into()));
            }
            if let Some(rtf) = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
            {
                formats.push(("text/rtf", rtf.into()));
            }
        }
        kind => bail!("Unsupported drag item type: {kind}"),
    }
    if formats.is_empty() {
        bail!("Item has no transferable content");
    }
    Ok(PreparedExternalDrag {
        formats: Arc::new(formats),
        files,
    })
}

fn file_uris(paths: &[String]) -> Result<Vec<u8>> {
    if paths.is_empty() {
        bail!("File list is empty");
    }
    let mut result = String::new();
    for path in paths {
        let file = Path::new(path);
        if !file.is_absolute() || !file.exists() {
            bail!("Drag source no longer exists: {path}");
        }
        let uri = url::Url::from_file_path(file)
            .map_err(|_| anyhow::anyhow!("Invalid file path: {path}"))?;
        result.push_str(uri.as_str());
        result.push_str("\r\n");
    }
    Ok(result.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_escape_delimiters_and_reject_missing_sources() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("a #é.txt");
        std::fs::write(&file, b"drag payload").unwrap();
        let uri =
            String::from_utf8(file_uris(&[file.to_str().unwrap().to_owned()]).unwrap()).unwrap();
        assert!(uri.ends_with("/a%20%23%C3%A9.txt\r\n"));
        std::fs::remove_file(&file).unwrap();
        assert!(file_uris(&[file.to_str().unwrap().to_owned()]).is_err());
        assert!(file_uris(&[]).is_err());
    }
}
