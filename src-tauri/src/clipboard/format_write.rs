//! 将数据库条目写回系统剪贴板（尽量保留原始格式）

use crate::database::ClipboardItem;
use arboard::Clipboard;

/// 将 ClipboardItem 写入系统剪贴板
pub fn write_item_to_clipboard(
    item: &ClipboardItem,
    clipboard: &mut Clipboard,
) -> Result<(), String> {
    match item.content_type.as_str() {
        "text" | "url" => write_plain_text(item, clipboard),
        "html" => write_html_item(item, clipboard),
        "rtf" => write_rtf_item(item, clipboard),
        "image" => {
            if let Some(ref path) = item.image_path {
                set_clipboard_image(path, clipboard)
            } else {
                Err("Item has no image path".to_string())
            }
        }
        "files" => {
            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json)
                    .map_err(|e| format!("Failed to parse file paths: {e}"))?;
                set_clipboard_files(&paths, clipboard)
            } else {
                Err("Item has no file paths".to_string())
            }
        }
        other => Err(format!("Unsupported content type: {other}")),
    }
}

fn write_plain_text(item: &ClipboardItem, clipboard: &mut Clipboard) -> Result<(), String> {
    let text = item
        .text_content
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "Item has no text content".to_string())?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to set clipboard text: {e}"))
}

fn write_html_item(item: &ClipboardItem, clipboard: &mut Clipboard) -> Result<(), String> {
    let alt = item_alt_text(item);

    if let Some(html) = item.html_content.as_deref().filter(|h| !h.is_empty()) {
        #[cfg(target_os = "windows")]
        if item.rtf_content.as_deref().is_some_and(|r| !r.is_empty()) {
            return write_windows_rich_formats(
                Some(html),
                item.rtf_content.as_deref(),
                alt.as_deref(),
            );
        }

        return clipboard
            .set_html(html.to_string(), alt)
            .map_err(|e| format!("Failed to set clipboard HTML: {e}"));
    }

    write_plain_text(item, clipboard)
}

fn write_rtf_item(item: &ClipboardItem, clipboard: &mut Clipboard) -> Result<(), String> {
    let alt = item_alt_text(item);

    #[cfg(target_os = "windows")]
    if let Some(rtf) = item.rtf_content.as_deref().filter(|r| !r.is_empty()) {
        // 统一使用 write_windows_rich_formats，html=None 时仅写 RTF + Unicode 文本
        return write_windows_rich_formats(item.html_content.as_deref(), Some(rtf), alt.as_deref());
    }

    write_plain_text(item, clipboard)
}

/// 提取条目可用的纯文本 fallback（HTML/RTF 写剪贴板时的 Unicode 伴生格式）
fn item_alt_text(item: &ClipboardItem) -> Option<String> {
    item.text_content
        .clone()
        .filter(|t| !t.is_empty())
        .or_else(|| {
            item.html_content
                .as_ref()
                .map(|h| strip_html_tags(h))
                .filter(|t| !t.is_empty())
        })
        .or_else(|| {
            item.preview
                .clone()
                .filter(|p| !p.is_empty() && !p.starts_with('['))
        })
}

/// 提取用于「纯文本粘贴」的字符串
pub fn item_plain_text(item: &ClipboardItem) -> Result<String, String> {
    if let Some(text) = item.text_content.as_ref().filter(|t| !t.is_empty()) {
        return Ok(text.clone());
    }

    match item.content_type.as_str() {
        "html" => item
            .html_content
            .as_ref()
            .map(|h| strip_html_tags(h))
            .filter(|t| !t.is_empty())
            .ok_or_else(|| "Item has no text content".to_string()),
        "rtf" => item_alt_text(item).ok_or_else(|| "Item has no text content".to_string()),
        "text" | "url" => Err("Item has no text content".to_string()),
        other => Err(format!(
            "Item type {other} has no plain text representation"
        )),
    }
}

/// 使用 arboard 将图片写入剪贴板（Windows 上写 CF_DIB，PS 兼容）
fn set_clipboard_image(path: &str, clipboard: &mut Clipboard) -> Result<(), String> {
    use arboard::ImageData;
    use std::borrow::Cow;

    let img = image::open(path)
        .map_err(|e| format!("Failed to load image from path: {e}"))?
        .to_rgba8();

    let (width, height) = img.dimensions();
    let image_data = ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(img.into_raw()),
    };

    clipboard
        .set_image(image_data)
        .map_err(|e| format!("Failed to set clipboard image: {e}"))
}

/// 使用 arboard 将文件列表写入剪贴板（Windows 上写 CF_HDROP）
fn set_clipboard_files(paths: &[String], clipboard: &mut Clipboard) -> Result<(), String> {
    use std::path::Path;

    let path_refs: Vec<&Path> = paths.iter().map(Path::new).collect();

    clipboard
        .set()
        .file_list(&path_refs)
        .map_err(|e| format!("Failed to set clipboard files: {e}"))
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Windows 剪贴板写入 ────────────────────────────────────────────

/// RAII guard: 确保 CloseClipboard 在任何退出路径（含 panic）都执行
#[cfg(target_os = "windows")]
struct ClipboardGuard;

#[cfg(target_os = "windows")]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::DataExchange::CloseClipboard().ok();
        }
    }
}

/// 写入多种富文本格式到剪贴板（RTF + CF_UNICODETEXT + CF_HTML）
/// 策略：先准备好所有格式的内存句柄，再一次性 EmptyClipboard + SetClipboardData，
/// 确保不会出现中途失败导致剪贴板只有部分格式的情况。
#[cfg(target_os = "windows")]
fn write_windows_rich_formats(
    html: Option<&str>,
    rtf: Option<&str>,
    plain_text: Option<&str>,
) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        EmptyClipboard, OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::core::PCSTR;

    // 预分配所有格式的内存句柄（在 EmptyClipboard 之前完成，失败时不会清空剪贴板）
    struct PreparedFormat {
        format: u32,
        hmem: windows::Win32::Foundation::HGLOBAL,
    }

    let mut prepared: Vec<PreparedFormat> = Vec::new();

    // 1. 准备 CF_UNICODETEXT
    if let Some(text) = plain_text.filter(|t| !t.is_empty()) {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * 2;
        let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) }
            .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
        let ptr = unsafe { GlobalLock(hmem) } as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr, byte_len);
            GlobalUnlock(hmem).map_err(|e| format!("GlobalUnlock failed: {e}"))?;
        }
        prepared.push(PreparedFormat { format: 13, hmem }); // CF_UNICODETEXT = 13
    }

    // 2. 准备 CF_HTML
    if let Some(html) = html.filter(|h| !h.is_empty()) {
        let cf_html = unsafe { RegisterClipboardFormatA(PCSTR(c"HTML Format".as_ptr().cast())) };
        if cf_html == 0 {
            return Err("Failed to register HTML clipboard format".to_string());
        }
        let packed = pack_html_for_clipboard(html);
        let data = packed.as_bytes();
        let size = data.len().saturating_add(1);
        let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, size) }
            .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
        let ptr = unsafe { GlobalLock(hmem) } as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        unsafe {
            if !data.is_empty() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            }
            *ptr.add(data.len()) = 0;
            GlobalUnlock(hmem).map_err(|e| format!("GlobalUnlock failed: {e}"))?;
        }
        prepared.push(PreparedFormat { format: cf_html, hmem });
    }

    // 3. 准备 CF_RTF
    if let Some(rtf) = rtf.filter(|r| !r.is_empty()) {
        let cf_rtf = unsafe { RegisterClipboardFormatA(PCSTR(c"Rich Text Format".as_ptr().cast())) };
        if cf_rtf == 0 {
            return Err("Failed to register RTF clipboard format".to_string());
        }
        let data = rtf.as_bytes();
        let size = data.len().saturating_add(1);
        let hmem = unsafe { GlobalAlloc(GMEM_MOVEABLE, size) }
            .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
        let ptr = unsafe { GlobalLock(hmem) } as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock failed".to_string());
        }
        unsafe {
            if !data.is_empty() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            }
            *ptr.add(data.len()) = 0;
            GlobalUnlock(hmem).map_err(|e| format!("GlobalUnlock failed: {e}"))?;
        }
        prepared.push(PreparedFormat { format: cf_rtf, hmem });
    }

    if prepared.is_empty() {
        return Ok(());
    }

    // 所有格式准备完毕，一次性写入
    unsafe {
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {e}"))?;
        let _clip_guard = ClipboardGuard;

        EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;

        for pf in &prepared {
            SetClipboardData(pf.format, Some(HANDLE(pf.hmem.0)))
                .map_err(|e| format!("SetClipboardData failed: {e}"))?;
            // 成功后系统接管内存，设置为 null 阻止 drop 中的 GlobalFree
        }
    }

    // 清理：成功的句柄已被系统接管（hmem 设为 null），失败的会被 GlobalFree
    std::mem::forget(prepared); // 所有句柄已交给系统

    Ok(())
}


/// 构建 Windows HTML Format 剪贴板 payload（含 StartHTML/Fragment 偏移头）
fn pack_html_for_clipboard(html: &str) -> String {
    let fragment = if html.contains("StartFragment") {
        html.to_string()
    } else {
        format!("<!--StartFragment-->{html}<!--EndFragment-->")
    };
    let source = format!("<html><body>{fragment}</body></html>");

    let frag_start_marker = "<!--StartFragment-->";
    let frag_end_marker = "<!--EndFragment-->";
    let frag_start_rel = source
        .find(frag_start_marker)
        .map(|i| i + frag_start_marker.len())
        .unwrap_or(0);
    let frag_end_rel = source.find(frag_end_marker).unwrap_or(source.len());

    let header_len = "Version:0.9\r\nStartHTML:000000000\r\nEndHTML:000000000\r\nStartFragment:000000000\r\nEndFragment:000000000\r\n"
        .len();
    let start_html = header_len;
    let end_html = start_html + source.len();
    let start_fragment = start_html + frag_start_rel;
    let end_fragment = start_html + frag_end_rel;

    format!(
        "Version:0.9\r\nStartHTML:{start_html:09}\r\nEndHTML:{end_html:09}\r\nStartFragment:{start_fragment:09}\r\nEndFragment:{end_fragment:09}\r\n{source}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_tags_basic() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
    }

    #[test]
    fn pack_html_for_clipboard_has_valid_offsets() {
        let packed = pack_html_for_clipboard("<b>Hi</b>");
        assert!(packed.starts_with("Version:0.9"));
        assert!(packed.contains("<!--StartFragment-->"));
        assert!(packed.contains("<b>Hi</b>"));
    }
}
