//! 将数据库条目写回系统剪贴板（尽量保留原始格式）

use crate::database::ClipboardItem;
use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard as ClipboardTrait, ClipboardContent as RsClipboardContent, ClipboardContext,
};

/// 将 ClipboardItem 写入系统剪贴板
pub fn write_item_to_clipboard(
    item: &ClipboardItem,
    ctx: &mut ClipboardContext,
) -> Result<(), String> {
    match item.content_type.as_str() {
        "text" | "url" => write_plain_text(item, ctx),
        "html" => write_html_item(item, ctx),
        "rtf" => write_rtf_item(item, ctx),
        "image" => {
            if let Some(ref path) = item.image_path {
                set_clipboard_image(path, ctx)
            } else {
                Err("Item has no image path".to_string())
            }
        }
        "files" => {
            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json)
                    .map_err(|e| format!("Failed to parse file paths: {e}"))?;
                set_clipboard_files(&paths, ctx)
            } else {
                Err("Item has no file paths".to_string())
            }
        }
        other => Err(format!("Unsupported content type: {other}")),
    }
}

fn write_plain_text(item: &ClipboardItem, ctx: &mut ClipboardContext) -> Result<(), String> {
    let text = item
        .text_content
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "Item has no text content".to_string())?;
    ctx.set_text(text.to_string())
        .map_err(|e| format!("Failed to set clipboard text: {e}"))
}

fn write_html_item(item: &ClipboardItem, ctx: &mut ClipboardContext) -> Result<(), String> {
    if item.html_content.as_deref().is_some_and(|h| !h.is_empty())
        || item.rtf_content.as_deref().is_some_and(|r| !r.is_empty())
    {
        return write_rich_item(item, ctx);
    }
    write_plain_text(item, ctx)
}

fn write_rtf_item(item: &ClipboardItem, ctx: &mut ClipboardContext) -> Result<(), String> {
    if item.rtf_content.as_deref().is_some_and(|r| !r.is_empty())
        || item.html_content.as_deref().is_some_and(|h| !h.is_empty())
    {
        return write_rich_item(item, ctx);
    }
    write_plain_text(item, ctx)
}

fn build_rich_contents(item: &ClipboardItem, include_rtf: bool) -> Vec<RsClipboardContent> {
    let mut contents: Vec<RsClipboardContent> = Vec::new();

    if let Some(text) = item_alt_text(item) {
        contents.push(RsClipboardContent::Text(text));
    }

    if let Some(html) = item.html_content.as_deref().filter(|h| !h.is_empty()) {
        // clipboard-rs v0.3.4 的 set() 会调用 plain_html_to_cf_html 包装 CF-HTML
        contents.push(RsClipboardContent::Html(html.to_string()));
    }

    if include_rtf && super::rtf_storage::should_write_rtf(item.rtf_content.as_deref()) {
        let raw = super::rtf_storage::decode_rtf_for_clipboard(item.rtf_content.as_ref().unwrap());
        if !raw.is_empty() {
            contents.push(RsClipboardContent::Other(
                "Rich Text Format".to_string(),
                raw,
            ));
        }
    }

    contents
}

fn rich_clipboard_verified(item: &ClipboardItem, ctx: &ClipboardContext) -> bool {
    if item.text_content.as_deref().is_some_and(|t| !t.is_empty())
        && ctx.get_text().ok().is_some_and(|t| !t.trim().is_empty())
    {
        return true;
    }

    if item.html_content.as_deref().is_some_and(|h| !h.is_empty())
        && ctx.get_html().ok().is_some_and(|h| !h.trim().is_empty())
    {
        return true;
    }

    super::rtf_storage::should_write_rtf(item.rtf_content.as_deref())
        && ctx
            .get_buffer("Rich Text Format")
            .ok()
            .is_some_and(|b| b.len() > 1)
}

/// 通过 clipboard-rs 一次性写入 Text + HTML + RTF（v0.3.4 内置 CF-HTML 包装 + OpenClipboard 重试）
fn write_rich_item(item: &ClipboardItem, ctx: &mut ClipboardContext) -> Result<(), String> {
    for include_rtf in [true, false] {
        let contents = build_rich_contents(item, include_rtf);
        if contents.is_empty() {
            continue;
        }

        ctx.set(contents)
            .map_err(|e| format!("Failed to set clipboard rich content: {e}"))?;

        if rich_clipboard_verified(item, ctx) {
            return Ok(());
        }
    }

    if let Some(text) = item_alt_text(item) {
        ctx.set_text(text)
            .map_err(|e| format!("Failed to set clipboard text: {e}"))?;
        return Ok(());
    }

    Err("Failed to set clipboard rich content: verification failed".to_string())
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

fn set_clipboard_image(path: &str, ctx: &mut ClipboardContext) -> Result<(), String> {
    use clipboard_rs::RustImageData;

    let img = image::open(path).map_err(|e| format!("Failed to load image from path: {e}"))?;
    let rgba_img = img.to_rgba8();
    let dynamic = image::DynamicImage::ImageRgba8(rgba_img);
    let image_data = RustImageData::from_dynamic_image(dynamic);

    ctx.set_image(image_data)
        .map_err(|e| format!("Failed to set clipboard image: {e}"))
}

fn set_clipboard_files(paths: &[String], ctx: &mut ClipboardContext) -> Result<(), String> {
    ctx.set_files(paths.to_vec())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_tags_basic() {
        assert_eq!(strip_html_tags("<p>Hello <b>world</b></p>"), "Hello world");
    }
}
