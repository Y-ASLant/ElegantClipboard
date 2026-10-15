use anyhow::{Context, Result, bail};
use clipboard_core::{History, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, database::ClipboardItem};
use std::{io::Read, path::Path, sync::Arc};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};

/// Owned bytes only: prepare on a worker, then transfer to the window thread.
#[derive(Clone, Debug)]
pub struct PreparedExternalDrag {
    pub(super) formats: Arc<Vec<(u16, Vec<u8>)>>,
}

/// Reads existing attachment files, but never reads the database or writes the clipboard.
pub fn prepare_external_drag(
    item: &ClipboardItem,
    staged_dir: &Path,
) -> Result<PreparedExternalDrag> {
    let mut formats = Vec::new();
    match item.content_type.as_str() {
        "files" => {
            let paths = History::file_paths_for_copy(item, staged_dir)?;
            formats.push((CF_HDROP.0, file_drop(&paths)?));
        }
        "image" => {
            let path = item.image_path.as_deref().context("图片文件路径缺失")?;
            formats.push((CF_HDROP.0, file_drop(&[path.to_owned()])?));
            // BMP without its 14-byte file header is CF_DIB. The encoder supplies
            // the bitmap header, palette and padded scanlines in Windows order.
            let image = decode_image(path)?;
            let mut bitmap = std::io::Cursor::new(Vec::new());
            // A 24-bit BI_RGB DIB is accepted by older OLE consumers too.
            image::DynamicImage::ImageRgb8(image.to_rgb8())
                .write_to(&mut bitmap, image::ImageFormat::Bmp)?;
            formats.push((CF_DIB.0, bitmap.into_inner().split_off(14)));
        }
        "text" | "url" | "html" | "rtf" => {
            if let Some(text) = item.text_content.as_deref().filter(|text| !text.is_empty()) {
                formats.push((CF_UNICODETEXT.0, unicode_text(text)?));
            }
            if let Some(html) = item.html_content.as_deref().filter(|html| !html.is_empty()) {
                formats.push((registered_format("HTML Format")?, html_format(html)?));
            }
            if let Some(rtf) = item
                .rtf_content
                .as_deref()
                .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
            {
                formats.push((registered_format("Rich Text Format")?, rtf));
            }
        }
        kind => bail!("记录类型 {kind} 不支持拖放"),
    }
    if formats.is_empty() {
        bail!("记录没有可拖放的内容");
    }
    Ok(PreparedExternalDrag {
        formats: Arc::new(formats),
    })
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

fn registered_format(name: &str) -> Result<u16> {
    Ok(clipboard_win::register_format(name)
        .context("注册拖放格式失败")?
        .get() as u16)
}

fn unicode_text(text: &str) -> Result<Vec<u8>> {
    if text.contains('\0') {
        bail!("拖放文本包含无效的空字符");
    }
    Ok(text
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect())
}

fn file_drop(paths: &[String]) -> Result<Vec<u8>> {
    if paths.is_empty() {
        bail!("拖放文件列表为空");
    }
    // DROPFILES: pFiles, POINT, fNC, fWide. Explicit serialization avoids padding.
    let mut bytes: Vec<u8> = [20u32, 0, 0, 0, 1]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    for path in paths {
        if !Path::new(path).is_absolute() || !Path::new(path).exists() {
            bail!("拖放源文件或文件夹已不存在：{path}");
        }
        bytes.extend(unicode_text(path)?);
    }
    bytes.extend([0, 0]);
    Ok(bytes)
}

fn html_format(fragment: &str) -> Result<Vec<u8>> {
    if fragment.contains('\0') {
        bail!("HTML 拖放内容包含无效的空字符");
    }
    let prefix = "<html><body><!--StartFragment-->";
    let suffix = "<!--EndFragment--></body></html>";
    let header = |start, end, fragment_start, fragment_end| {
        format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{end:010}\r\nStartFragment:{fragment_start:010}\r\nEndFragment:{fragment_end:010}\r\n"
        )
    };
    let start = header(0, 0, 0, 0).len();
    let fragment_start = start + prefix.len();
    let fragment_end = fragment_start + fragment.len();
    let end = fragment_end + suffix.len();
    let mut bytes = header(start, end, fragment_start, fragment_end).into_bytes();
    bytes.extend_from_slice(prefix.as_bytes());
    bytes.extend_from_slice(fragment.as_bytes());
    bytes.extend_from_slice(suffix.as_bytes());
    bytes.push(0);
    Ok(bytes)
}
