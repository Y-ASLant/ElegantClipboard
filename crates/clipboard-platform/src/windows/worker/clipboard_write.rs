use super::image_tmp::ensure_temp_png;
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::database::ClipboardItem;
use clipboard_rs::Clipboard;
use std::{path::Path, thread};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

// clipboard-rs opens/closes the clipboard inside each setter and getter. Do not
// wrap those calls in another clipboard guard: closing a nested guard releases
// the OS clipboard lock. Verify the content across a stable sequence instead.
pub(super) fn verified_copy_sequence(
    mut sequence: impl FnMut() -> u32,
    mut matches: impl FnMut() -> bool,
) -> Result<u32> {
    for _ in 0..4 {
        let before = sequence();
        if before == 0 {
            bail!("无法确认复制内容的剪贴板序列号，请重试");
        }
        let content_matches = matches();
        let after = sequence();
        if before == after {
            if !content_matches {
                bail!("复制内容写回后校验失败，请重试");
            }
            return Ok(after);
        }
        thread::yield_now();
    }
    bail!("复制内容写回后剪贴板持续变化，请重试")
}

/// 已打开的 clipboard_win 会话内写全部额外原生格式；返回失败格式名。
/// 单条失败不中断主格式（额外格式是尽力而为的保真补充）。
pub(super) fn write_native_formats_locked(formats: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut failed = Vec::new();
    for (name, data) in formats {
        let Some(format) = clipboard_win::register_format(name) else {
            failed.push(name.clone());
            continue;
        };
        if clipboard_win::raw::set_without_clear(format.get(), data).is_err() {
            failed.push(name.clone());
        }
    }
    failed
}

/// 富文本写回：单 guard 会话内 text→html→rtf→extras 顺序写入。
/// extras 失败不中断主格式；返回 extras 失败名列表（序列号经 ignored_sequence 记录）。
pub(super) fn write_rich_clipboard(
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
    extras: &[(String, Vec<u8>)],
    ignored_sequence: &mut u32,
) -> Result<Vec<String>> {
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    if text.is_none() && html.is_none() && rtf.is_none() {
        bail!("富文本记录没有可写回的有效格式");
    }

    // clipboard-rs::set suppresses failures of individual formats. Keep one
    // clipboard open so every requested format is written or reported as failed.
    let mut touched = false;
    let mut extras_failed: Vec<String> = Vec::new();
    let result: Result<()> = (|| {
        let clipboard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
        clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
        touched = true;
        if let Some(text) = text {
            clipboard_win::raw::set_string_with(text, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入纯文本格式失败：{error}"))?;
        }
        if let Some(html) = html {
            let format = clipboard_win::register_format("HTML Format")
                .context("注册 HTML 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_html_with(format, html, clipboard_win::options::NoClear)
                .map_err(|error| anyhow!("写入 HTML 格式失败：{error}"))?;
        }
        if let Some(rtf) = rtf {
            let format = clipboard_win::register_format("Rich Text Format")
                .context("注册 RTF 剪贴板格式失败")?
                .get();
            clipboard_win::raw::set_without_clear(format, rtf)
                .map_err(|error| anyhow!("写入 RTF 格式失败：{error}"))?;
        }
        extras_failed = write_native_formats_locked(extras);
        drop(clipboard);
        Ok(())
    })();
    if touched {
        // Capture the final sequence after the clipboard guard has closed, even
        // when a later format failed and left a partial write behind.
        *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    }
    result?;
    Ok(extras_failed)
}

/// 图片写回：内容寻址临时 PNG 前置 CF_HDROP，使纯图片可直接以文件粘贴。
/// 单 guard 会话内：empty → CF_HDROP → CF_DIB → PNG。
/// `content_hash` 是记录已存的哈希（"image:<hex>" 形式），取 hex 段命名临时文件。
pub(super) fn write_image_with_file(
    item: &ClipboardItem,
    data_dir: &Path,
    png: &[u8],
    pixels: &[u8],
    width: u32,
    height: u32,
    ignored_sequence: &mut u32,
) -> Result<()> {
    let dib = build_cf_dib(pixels, width, height)?;
    let hex = item.content_hash.trim_start_matches("image:");
    let temp_path = ensure_temp_png(data_dir, hex, png)?;
    let temp_string = temp_path.to_string_lossy().into_owned();
    let mut touched = false;
    let result = (|| {
        let clipboard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
        clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
        touched = true;
        // CF_HDROP：文件路径列表（托管 PNG 临时文件）。
        clipboard_win::raw::set_file_list_with(
            std::slice::from_ref(&temp_string),
            clipboard_win::options::NoClear,
        )
        .map_err(|error| anyhow!("写入图片文件格式失败：{error}"))?;
        // CF_DIB：手工组 BITMAPINFOHEADER + 自底向上 BGRA 像素。
        clipboard_win::raw::set_without_clear(8, &dib)
            .map_err(|error| anyhow!("写入图片位图格式失败：{error}"))?;
        // PNG 主格式。
        let png_format = clipboard_win::register_format("PNG")
            .context("注册 PNG 剪贴板格式失败")?
            .get();
        clipboard_win::raw::set_without_clear(png_format, png)
            .map_err(|error| anyhow!("写入 PNG 格式失败：{error}"))?;
        drop(clipboard);
        Ok(())
    })();
    if touched {
        *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    }
    result
}

/// CF_DIB 字节：40 字节 BITMAPINFOHEADER + 32 位像素（自底向上，BGRA）。
/// `pixels` 是扁平 RGBA 缓冲（每像素 4 字节，行优先），尺寸须来自实际解码的图片。
pub(super) fn build_cf_dib(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        bail!("图片尺寸不能为空");
    }
    let width_i32 = i32::try_from(width).context("图片宽度超过位图限制")?;
    let height_i32 = i32::try_from(height).context("图片高度超过位图限制")?;
    let row_bytes = usize::try_from(width)?
        .checked_mul(4)
        .context("图片行长度溢出")?;
    let image_bytes = row_bytes
        .checked_mul(usize::try_from(height)?)
        .context("图片缓冲长度溢出")?;
    let image_size = u32::try_from(image_bytes).context("图片像素超过位图长度限制")?;
    if pixels.len() != image_bytes {
        bail!("图片像素长度与实际尺寸不符");
    }
    let dib_size = image_bytes.checked_add(40).context("位图长度溢出")?;
    let mut dib = Vec::new();
    dib.try_reserve_exact(dib_size)
        .context("位图缓冲分配失败")?;
    // BITMAPINFOHEADER
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&width_i32.to_le_bytes());
    // 正高度 = 自底向上。
    dib.extend_from_slice(&height_i32.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    dib.extend_from_slice(&image_size.to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    dib.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    for row in pixels.chunks_exact(row_bytes).rev() {
        for rgba in row.as_chunks::<4>().0 {
            dib.extend_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
    }
    Ok(dib)
}

pub(super) fn rich_clipboard_matches(
    clipboard: &clipboard_rs::ClipboardContext,
    item: &ClipboardItem,
    rtf: Option<&[u8]>,
) -> bool {
    let html = item
        .html_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let text = item
        .text_content
        .as_deref()
        .filter(|value| !value.is_empty());
    let html_matches = html.is_none_or(|expected| {
        crate::service::read_html::read_html(clipboard)
            .ok()
            .is_some_and(|actual| actual.contains(expected))
    });
    let rtf_matches = rtf.is_none_or(|expected| {
        clipboard
            .get_buffer("Rich Text Format")
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    let text_matches = text.is_none_or(|expected| {
        clipboard
            .get_text()
            .ok()
            .is_some_and(|actual| actual == expected)
    });
    html_matches && rtf_matches && text_matches
}

/// 纯文本路径的额外格式追加：独立 clipboard_win 会话，NoClear 不清空现有内容。
/// 返回失败格式名。
pub(super) fn write_extras_session(extras: &[(String, Vec<u8>)]) -> Result<Vec<String>> {
    let clipboard = clipboard_win::Clipboard::new_attempts(10)
        .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
    let failed = write_native_formats_locked(extras);
    drop(clipboard);
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_confirmation_requires_matching_content_and_a_nonzero_sequence() {
        // An external writer can replace our content before the first sequence
        // read. A stable sequence alone must not report that copy as successful.
        assert!(verified_copy_sequence(|| 42, || false).is_err());
        assert!(verified_copy_sequence(|| 0, || true).is_err());
        assert_eq!(verified_copy_sequence(|| 42, || true).unwrap(), 42);
    }

    #[test]
    fn cf_dib_rejects_dimensions_and_buffer_lengths_that_cannot_describe_rgba() {
        for (width, height, len) in [
            (0, 1, 0),
            (1, 0, 0),
            (u32::MAX, 1, 0),
            (1, u32::MAX, 0),
            (i32::MAX as u32, i32::MAX as u32, 0),
            (65_536, 16_384, 0), // biSizeImage cannot hold 4 GiB.
            (2, 2, 12),          // Truncated or stale larger dimensions.
            (1, 1, 5),           // Excess bytes must not produce an inconsistent header.
            (1, 1, 3),           // Partial pixel.
        ] {
            assert!(build_cf_dib(&vec![0; len], width, height).is_err());
        }
    }

    #[test]
    fn cf_dib_single_pixel_preserves_alpha() {
        let dib = build_cf_dib(&[1, 2, 3, 4], 1, 1).unwrap();
        assert_eq!(&dib[40..], &[3, 2, 1, 4]);
        assert_eq!(&dib[20..24], &4u32.to_le_bytes());
    }

    #[test]
    fn cf_dib_layout_matches_header_and_bottom_up_bgra_pixels() {
        // 2×2 像素扁平 RGBA：上行 [p0, p1]，下行 [p2, p3]。自底向上存储时下行在先。
        let pixels: Vec<u8> = [
            [0x11, 0x22, 0x33, 0xFF], // p0 上左
            [0x44, 0x55, 0x66, 0xFF], // p1 上右
            [0x77, 0x88, 0x99, 0xFF], // p2 下左
            [0xAA, 0xBB, 0xCC, 0xFF], // p3 下右
        ]
        .iter()
        .flat_map(|pixel| pixel.iter().copied())
        .collect();
        let dib = build_cf_dib(&pixels, 2, 2).unwrap();
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(&dib[20..24], &16u32.to_le_bytes());
        assert_eq!(&dib[..4], &40u32.to_le_bytes());
        assert_eq!(&dib[4..8], &2i32.to_le_bytes());
        assert_eq!(&dib[8..12], &2i32.to_le_bytes());
        // 自底向上：最底行（p2, p3）在先；行内自左向右；BGRA 字节序。
        assert_eq!(&dib[40..44], &[0x99, 0x88, 0x77, 0xFF]);
        assert_eq!(&dib[44..48], &[0xCC, 0xBB, 0xAA, 0xFF]);
        assert_eq!(&dib[48..52], &[0x33, 0x22, 0x11, 0xFF]);
        assert_eq!(&dib[52..56], &[0x66, 0x55, 0x44, 0xFF]);
    }
}
