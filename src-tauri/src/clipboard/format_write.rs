//! 将数据库条目写回系统剪贴板（尽量保留原始格式）

use crate::database::ClipboardItem;
use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard as ClipboardTrait, ClipboardContent as RsClipboardContent, ClipboardContext,
};
use tracing::{debug, warn};

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
    match ctx.set_text(text.to_string()) {
        Ok(()) => {
            debug!(id = item.id, len = text.len(), "write_plain_text: ok");
            Ok(())
        }
        Err(e) => {
            warn!(id = item.id, len = text.len(), error = %e, "write_plain_text: failed");
            Err(format!("Failed to set clipboard text: {e}"))
        }
    }
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

    if include_rtf
        && let Some(rtf) = item
            .rtf_content
            .as_deref()
            .filter(|r| super::rtf_storage::should_write_rtf(Some(r)))
    {
        let raw = super::rtf_storage::decode_rtf_for_clipboard(rtf);
        if !raw.is_empty() {
            contents.push(RsClipboardContent::Other(
                "Rich Text Format".to_string(),
                raw,
            ));
        }
    }

    contents
}

fn rich_clipboard_verify_detail(item: &ClipboardItem, ctx: &ClipboardContext) -> (bool, String) {
    let mut parts = Vec::new();

    if item.text_content.as_deref().is_some_and(|t| !t.is_empty()) {
        let ok = ctx.get_text().ok().is_some_and(|t| !t.trim().is_empty());
        parts.push(format!("text={}", if ok { "ok" } else { "fail" }));
        if ok {
            return (true, parts.join(", "));
        }
    }

    if item.html_content.as_deref().is_some_and(|h| !h.is_empty()) {
        let ok = ctx.get_html().ok().is_some_and(|h| !h.trim().is_empty());
        parts.push(format!("html={}", if ok { "ok" } else { "fail" }));
        if ok {
            return (true, parts.join(", "));
        }
    }

    if super::rtf_storage::should_write_rtf(item.rtf_content.as_deref()) {
        let ok = ctx
            .get_buffer("Rich Text Format")
            .ok()
            .is_some_and(|b| b.len() > 1);
        parts.push(format!("rtf={}", if ok { "ok" } else { "fail" }));
        if ok {
            return (true, parts.join(", "));
        }
    }

    (false, parts.join(", "))
}

fn rich_write_meta(item: &ClipboardItem) -> (bool, usize, usize, usize) {
    (
        super::rtf_storage::should_write_rtf(item.rtf_content.as_deref()),
        item.html_content.as_deref().map_or(0, str::len),
        item.rtf_content.as_deref().map_or(0, str::len),
        item.text_content.as_deref().map_or(0, str::len),
    )
}

fn rich_contents_summary(contents: &[RsClipboardContent]) -> String {
    contents
        .iter()
        .map(|c| match c {
            RsClipboardContent::Text(_) => "text",
            RsClipboardContent::Html(_) => "html",
            RsClipboardContent::Rtf(_) => "rtf",
            RsClipboardContent::Other(name, _) => name.as_str(),
            RsClipboardContent::Files(_) => "files",
            _ => "other",
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// 通过 clipboard-rs 一次性写入 Text + HTML + RTF（v0.3.4 内置 CF-HTML 包装 + OpenClipboard 重试）
fn write_rich_item(item: &ClipboardItem, ctx: &mut ClipboardContext) -> Result<(), String> {
    let (has_b64_rtf, html_len, rtf_stored_len, text_len) = rich_write_meta(item);
    debug!(
        id = item.id,
        content_type = %item.content_type,
        has_b64_rtf,
        html_len,
        rtf_stored_len,
        text_len,
        "write_rich_item: start"
    );

    for include_rtf in [true, false] {
        let stage = if include_rtf { "with_rtf" } else { "no_rtf" };
        let contents = build_rich_contents(item, include_rtf);
        if contents.is_empty() {
            debug!(id = item.id, stage, "write_rich_item: skip empty contents");
            continue;
        }

        let formats = rich_contents_summary(&contents);
        match ctx.set(contents) {
            Ok(()) => {
                let (verified, verify_detail) = rich_clipboard_verify_detail(item, ctx);
                debug!(
                    id = item.id,
                    stage,
                    formats = %formats,
                    set = "ok",
                    verified,
                    verify_detail = %verify_detail,
                    "write_rich_item: set done"
                );
                if verified {
                    return Ok(());
                }
            }
            Err(e) => {
                debug!(
                    id = item.id,
                    stage,
                    formats = %formats,
                    set = "err",
                    error = %e,
                    "write_rich_item: set failed"
                );
            }
        }
    }

    if let Some(text) = item_alt_text(item) {
        let alt_len = text.len();
        match ctx.set_text(text) {
            Ok(()) => {
                debug!(id = item.id, alt_len, "write_rich_item: text fallback ok");
                return Ok(());
            }
            Err(e) => {
                warn!(
                    id = item.id,
                    alt_len,
                    error = %e,
                    "write_rich_item: text fallback failed"
                );
                return Err(format!("Failed to set clipboard text: {e}"));
            }
        }
    }

    warn!(
        id = item.id,
        content_type = %item.content_type,
        has_b64_rtf,
        html_len,
        rtf_stored_len,
        text_len,
        "write_rich_item: verification failed"
    );
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

    let img = image::open(path).map_err(|e| {
        warn!(path, error = %e, "set_clipboard_image: failed to load");
        format!("Failed to load image from path: {e}")
    })?;
    let rgba_img = img.to_rgba8();
    let (w, h) = rgba_img.dimensions();
    let dynamic = image::DynamicImage::ImageRgba8(rgba_img);
    let image_data = RustImageData::from_dynamic_image(dynamic);

    match ctx.set_image(image_data) {
        Ok(()) => {
            debug!(path, w, h, "set_clipboard_image: ok");
            Ok(())
        }
        Err(e) => {
            warn!(path, w, h, error = %e, "set_clipboard_image: failed");
            Err(format!("Failed to set clipboard image: {e}"))
        }
    }
}

fn set_clipboard_files(paths: &[String], ctx: &mut ClipboardContext) -> Result<(), String> {
    match ctx.set_files(paths.to_vec()) {
        Ok(()) => {
            debug!(count = paths.len(), "set_clipboard_files: ok");
            Ok(())
        }
        Err(e) => {
            warn!(count = paths.len(), error = %e, "set_clipboard_files: failed");
            Err(format!("Failed to set clipboard files: {e}"))
        }
    }
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
