use anyhow::{Context, Result, bail};
use clipboard_core::{History, MAX_IMAGE_BYTES, database::ClipboardItem};
use std::{path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub struct PreparedExternalDrag(pub(super) Arc<Vec<DragItem>>);

#[derive(Debug)]
pub(super) struct DragItem {
    pub path: Option<String>,
    pub formats: Vec<(&'static str, Vec<u8>)>,
}

pub fn prepare_external_drag(item: &ClipboardItem) -> Result<PreparedExternalDrag> {
    let mut items = Vec::new();
    match item.content_type.as_str() {
        "files" => {
            for path in History::file_paths_for_copy(item)? {
                validate_path(&path)?;
                items.push(DragItem {
                    path: Some(path),
                    formats: Vec::new(),
                });
            }
        }
        "image" => {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            validate_path(path)?;
            let file = std::fs::File::open(path)?;
            if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_IMAGE_BYTES as u64 {
                bail!("拖放图片无效或超过 50 MiB");
            }
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_IMAGE_BYTES {
                bail!("拖放图片超过 50 MiB")
            }
            let format = match image::guess_format(&bytes)? {
                image::ImageFormat::Png => "public.png",
                image::ImageFormat::Jpeg => "public.jpeg",
                image::ImageFormat::Tiff => "public.tiff",
                _ => bail!("macOS 拖放图片需要 PNG、JPEG 或 TIFF 格式"),
            };
            items.push(DragItem {
                path: Some(path.to_owned()),
                formats: vec![(format, bytes)],
            });
        }
        "text" | "url" | "html" | "rtf" => {
            let mut formats = Vec::new();
            if let Some(text) = item.text_content.as_deref().filter(|text| !text.is_empty()) {
                formats.push(("public.utf8-plain-text", text.as_bytes().to_vec()));
                if item.content_type == "url" {
                    formats.push(("public.url", text.as_bytes().to_vec()));
                }
            }
            if let Some(html) = item.html_content.as_deref().filter(|text| !text.is_empty()) {
                formats.push(("public.html", html.as_bytes().to_vec()));
            }
            if let Some(rtf) = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
            {
                formats.push(("public.rtf", rtf));
            }
            if !formats.is_empty() {
                items.push(DragItem {
                    path: None,
                    formats,
                });
            }
        }
        kind => bail!("记录类型 {kind} 不支持拖放"),
    }
    if items.is_empty() {
        bail!("记录没有可拖放的内容")
    }
    Ok(PreparedExternalDrag(Arc::new(items)))
}

fn validate_path(path: &str) -> Result<()> {
    if !Path::new(path).is_absolute() || !Path::new(path).exists() {
        bail!("拖放源文件或文件夹已不存在：{path}");
    }
    Ok(())
}
