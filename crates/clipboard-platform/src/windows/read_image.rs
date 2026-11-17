//! Read clipboard images with format fallback.
//!
//! `clipboard-rs` prefers PNG and fails the whole read when that buffer is
//! incomplete (`failed to fill whole buffer`), even if CF_DIB / CF_DIBV5 is valid.
//! Try PNG → DIBV5 → DIB and only error after every available format fails.

use super::CapturedClipboard;
use super::read_content::retry_clipboard_read;
use anyhow::{Result, anyhow, bail};
use clipboard_core::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
use clipboard_rs::common::{RustImage, RustImageData};
use clipboard_rs::{Clipboard, ClipboardContext};
use clipboard_win::formats;
use std::io::Cursor;

pub(super) fn read_image(clipboard: &ClipboardContext) -> Result<CapturedClipboard> {
    let mut last_error: Option<anyhow::Error> = None;

    if png_available() {
        match retry_clipboard_read(|| clipboard.get_buffer("PNG")) {
            Ok(bytes) if !bytes.is_empty() => match image_from_bytes(&bytes) {
                Ok(image) => return captured_from_image(image),
                Err(error) => last_error = Some(error),
            },
            Ok(_) => {}
            Err(error) => last_error = Some(anyhow!("{error}")),
        }
    }

    if clipboard_win::is_format_avail(formats::CF_DIBV5) {
        match retry_clipboard_read(|| {
            clipboard_win::get_clipboard(formats::RawData(formats::CF_DIBV5))
                .map_err(|code| format!("OSError({code})"))
        }) {
            Ok(data) if !data.is_empty() => match image_from_dibv5(&data) {
                Ok(image) => return captured_from_image(image),
                Err(error) => last_error = Some(error),
            },
            Ok(_) => {}
            Err(error) => last_error = Some(anyhow!("{error}")),
        }
    }

    if clipboard_win::is_format_avail(formats::CF_DIB) {
        match retry_clipboard_read(|| {
            clipboard_win::get_clipboard(formats::Bitmap).map_err(|code| format!("OSError({code})"))
        }) {
            Ok(data) if !data.is_empty() => match image_from_bytes(&data) {
                Ok(image) => return captured_from_image(image),
                Err(error) => last_error = Some(error),
            },
            Ok(_) => {}
            Err(error) => last_error = Some(anyhow!("{error}")),
        }
    }

    match last_error {
        Some(error) => Err(anyhow!("读取剪贴板图片失败：{error}")),
        None => Err(anyhow!("读取剪贴板图片失败：No image data in clipboard")),
    }
}

fn png_available() -> bool {
    clipboard_win::register_format("PNG")
        .is_some_and(|format| clipboard_win::is_format_avail(format.get()))
}

fn image_from_bytes(bytes: &[u8]) -> Result<RustImageData> {
    RustImageData::from_bytes(bytes).map_err(|error| anyhow!("{error}"))
}

fn image_from_dibv5(data: &[u8]) -> Result<RustImageData> {
    let decoder = image::codecs::bmp::BmpDecoder::new_without_file_header(Cursor::new(data))
        .map_err(|error| anyhow!("{error}"))?;
    let image = image::DynamicImage::from_decoder(decoder).map_err(|error| anyhow!("{error}"))?;
    Ok(RustImageData::from_dynamic_image(image))
}

fn captured_from_image(image: RustImageData) -> Result<CapturedClipboard> {
    let (width, height) = image.get_size();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        bail!("图片尺寸无效或超过 2500 万像素");
    }
    let png = image
        .to_png()
        .map_err(|error| anyhow!("编码剪贴板图片失败：{error}"))?;
    if png.get_bytes().len() > MAX_IMAGE_BYTES {
        bail!("图片超过 50 MiB，未保存");
    }
    Ok(CapturedClipboard::Image {
        png: png.get_bytes().to_vec(),
        width,
        height,
    })
}

/// Incomplete delayed-render buffers and empty handles are not user-facing faults.
pub(super) fn is_missing_image_data(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    [
        "OSError(0)",
        "failed to fill whole buffer",
        "No image data in clipboard",
        "UnexpectedEof",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::is_missing_image_data;
    use anyhow::anyhow;

    #[test]
    fn incomplete_png_decode_is_treated_as_missing_data() {
        assert!(is_missing_image_data(&anyhow!(
            "读取剪贴板图片失败：failed to fill whole buffer"
        )));
        assert!(is_missing_image_data(&anyhow!("OSError(0)")));
        assert!(!is_missing_image_data(&anyhow!(
            "编码剪贴板图片失败：out of memory"
        )));
    }
}
