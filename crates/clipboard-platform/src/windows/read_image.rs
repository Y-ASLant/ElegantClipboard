//! Windows clipboard image read with format fallback.
//!
//! `clipboard-rs` prefers PNG and aborts when that buffer is incomplete
//! (`failed to fill whole buffer`), even if CF_DIB / CF_DIBV5 is valid.
//! Order: PNG → DIBV5 → DIB.

use super::CapturedClipboard;
use super::read_content::retry_clipboard_read;
use anyhow::{Result, anyhow, bail};
use clipboard_core::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
use clipboard_rs::common::{RustImage, RustImageData};
use clipboard_rs::{Clipboard, ClipboardContext};
use clipboard_win::formats;
use std::io::Cursor;

pub(super) fn read_image(clipboard: &ClipboardContext) -> Result<CapturedClipboard> {
    let mut last_error = None;

    if png_available()
        && let Some(image) = attempt(
            &mut last_error,
            || clipboard.get_buffer("PNG"),
            decode_encoded_image,
        )
    {
        return Ok(image);
    }

    if clipboard_win::is_format_avail(formats::CF_DIBV5)
        && let Some(image) = attempt(
            &mut last_error,
            || {
                clipboard_win::get_clipboard(formats::RawData(formats::CF_DIBV5))
                    .map_err(|code| format!("OSError({code})"))
            },
            decode_dibv5,
        )
    {
        return Ok(image);
    }

    if clipboard_win::is_format_avail(formats::CF_DIB)
        && let Some(image) = attempt(
            &mut last_error,
            || {
                clipboard_win::get_clipboard(formats::Bitmap)
                    .map_err(|code| format!("OSError({code})"))
            },
            decode_encoded_image,
        )
    {
        return Ok(image);
    }

    Err(anyhow!(
        "读取剪贴板图片失败：{}",
        last_error.unwrap_or_else(|| "No image data in clipboard".into())
    ))
}

fn attempt<E: ToString>(
    last_error: &mut Option<String>,
    read: impl FnMut() -> Result<Vec<u8>, E>,
    decode: impl FnOnce(&[u8]) -> Result<RustImageData>,
) -> Option<CapturedClipboard> {
    match retry_clipboard_read(read) {
        Ok(bytes) if !bytes.is_empty() => match decode(&bytes).and_then(captured_from_image) {
            Ok(image) => Some(image),
            Err(error) => {
                *last_error = Some(error.to_string());
                None
            }
        },
        Ok(_) => None,
        Err(error) => {
            *last_error = Some(error.to_string());
            None
        }
    }
}

fn png_available() -> bool {
    clipboard_win::register_format("PNG")
        .is_some_and(|format| clipboard_win::is_format_avail(format.get()))
}

fn decode_encoded_image(bytes: &[u8]) -> Result<RustImageData> {
    RustImageData::from_bytes(bytes).map_err(|error| anyhow!("{error}"))
}

fn decode_dibv5(data: &[u8]) -> Result<RustImageData> {
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

/// Delayed-render empty handles and incomplete buffers are not user-facing faults.
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
