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

/// 图片写回：先生成 24 位 CF_DIB，再打开剪贴板。QQ 的「以图片形式发送」要这份现成位图。
/// 同时写 PNG 和临时文件 CF_HDROP。单 guard：empty → CF_DIB → PNG → CF_HDROP。
pub(super) fn write_image_with_file(
    item: &ClipboardItem,
    data_dir: &Path,
    png: &[u8],
    ignored_sequence: &mut u32,
) -> Result<()> {
    let dib = crate::cf_dib::from_png(png)?;
    let hex = item.content_hash.trim_start_matches("image:");
    let temp_path = ensure_temp_png(data_dir, hex, png)?;
    let temp_string = temp_path.to_string_lossy().into_owned();
    let mut touched = false;
    let result = (|| {
        let clipboard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|error| anyhow!("打开剪贴板失败：{error}"))?;
        clipboard_win::raw::empty().map_err(|error| anyhow!("清空剪贴板失败：{error}"))?;
        touched = true;
        clipboard_win::raw::set_without_clear(8, &dib)
            .map_err(|error| anyhow!("写入图片位图格式失败：{error}"))?;
        let png_format = clipboard_win::register_format("PNG")
            .context("注册 PNG 剪贴板格式失败")?
            .get();
        clipboard_win::raw::set_without_clear(png_format, png)
            .map_err(|error| anyhow!("写入 PNG 格式失败：{error}"))?;
        clipboard_win::raw::set_file_list_with(
            std::slice::from_ref(&temp_string),
            clipboard_win::options::NoClear,
        )
        .map_err(|error| anyhow!("写入图片文件格式失败：{error}"))?;
        drop(clipboard);
        Ok(())
    })();
    if touched {
        *ignored_sequence = unsafe { GetClipboardSequenceNumber() };
    }
    result
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
}
