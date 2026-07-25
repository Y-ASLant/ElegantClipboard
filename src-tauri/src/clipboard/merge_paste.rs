//! 合并粘贴：聚合多条记录的文本/富文本/文件格式后一次性写入剪贴板。

use super::file_clipboard::{
    decode_payload, merge_file_paths, resolve_item_paths, write_payload_extras,
};
use super::format_write::{item_alt_text, rich_contents_summary};
use super::rtf_storage;
use crate::database::ClipboardItem;
use clipboard_rs::{
    Clipboard as ClipboardTrait, ClipboardContent as RsClipboardContent, ClipboardContext,
};
use std::path::Path;
use tracing::debug;

pub fn merge_items_to_clipboard(
    items: &[ClipboardItem],
    separator: &str,
    ctx: &mut ClipboardContext,
) -> Result<(), String> {
    if items.is_empty() {
        return Err("未选择条目".to_string());
    }

    let mut contents: Vec<RsClipboardContent> = Vec::new();

    let file_inputs: Vec<_> = items
        .iter()
        .filter(|i| i.content_type == "files")
        .map(|i| (i.file_paths.as_deref(), i.file_payload.as_deref()))
        .collect();
    let mut merged_paths = merge_file_paths(&file_inputs);

    // 图片条目：将缓存图片路径并入文件列表（便于粘贴到接受文件的 target）
    for item in items {
        if item.content_type == "image"
            && let Some(ref path) = item.image_path
            && Path::new(path).exists()
            && !merged_paths.iter().any(|p| p == path)
        {
            merged_paths.push(path.clone());
        }
    }

    if !merged_paths.is_empty() {
        contents.push(RsClipboardContent::Files(merged_paths));
    }

    let text_parts: Vec<String> = items.iter().filter_map(extract_merge_text).collect();
    if !text_parts.is_empty() {
        contents.push(RsClipboardContent::Text(text_parts.join(separator)));
    }

    let html_parts: Vec<String> = items
        .iter()
        .filter_map(|item| {
            item.html_content
                .as_ref()
                .filter(|h| !h.is_empty())
                .cloned()
        })
        .collect();
    if !html_parts.is_empty() {
        contents.push(RsClipboardContent::Html(html_parts.join(separator)));
    }

    let rtf_decoded: Vec<Vec<u8>> = items
        .iter()
        .filter_map(|item| {
            item.rtf_content
                .as_ref()
                .filter(|r| rtf_storage::should_write_rtf(Some(r.as_str())))
                .map(|r| rtf_storage::decode_rtf_for_clipboard(r))
                .filter(|b| !b.is_empty())
        })
        .collect();
    if rtf_decoded.len() == 1 {
        contents.push(RsClipboardContent::Other(
            "Rich Text Format".to_string(),
            rtf_decoded[0].clone(),
        ));
    }

    if contents.is_empty() {
        return Err("选中的项目没有可合并的内容".to_string());
    }

    let formats = rich_contents_summary(&contents);

    ctx.clear()
        .map_err(|e| format!("Failed to clear clipboard: {e}"))?;
    ctx.set(contents)
        .map_err(|e| format!("Failed to set merged clipboard: {e}"))?;

    // 合并文件时写入首个文件条目的伴生格式（不含 raw HDROP，路径已合并重建）
    if !file_inputs.is_empty()
        && let Some((_, payload_raw)) = file_inputs.first()
    {
        let payload = decode_payload(*payload_raw);
        write_payload_extras(ctx, payload.as_ref())?;
    }

    debug!("Merged paste: {} item(s), formats={formats}", items.len(),);
    Ok(())
}

fn extract_merge_text(item: &ClipboardItem) -> Option<String> {
    if let Some(text) = item.text_content.as_ref().filter(|t| !t.is_empty()) {
        return Some(text.clone());
    }
    if item.content_type == "files" {
        let resolved = resolve_item_paths(item.file_paths.as_deref(), item.file_payload.as_deref());
        if !resolved.is_empty() {
            return Some(resolved.join("\n"));
        }
    }
    if item.content_type == "image" {
        return item.image_path.as_ref().filter(|p| !p.is_empty()).cloned();
    }
    item_alt_text(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::ClipboardItem;

    fn item(id: i64, content_type: &str, text: Option<&str>, paths: Option<&str>) -> ClipboardItem {
        ClipboardItem {
            id,
            content_type: content_type.to_string(),
            text_content: text.map(str::to_string),
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: paths.map(str::to_string),
            file_payload: None,
            content_hash: "h".into(),
            semantic_hash: "h".into(),
            preview: None,
            byte_size: 0,
            image_width: None,
            image_height: None,
            is_pinned: false,
            is_favorite: false,
            favorite_order: 0,
            sort_order: id,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
            access_count: 0,
            last_accessed_at: None,
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
            group_id: None,
            files_valid: None,
        }
    }

    #[test]
    fn extract_merge_text_prefers_text_content() {
        let i = item(1, "text", Some("hello"), None);
        assert_eq!(extract_merge_text(&i), Some("hello".into()));
    }

    #[test]
    fn extract_merge_text_from_files() {
        let i = item(1, "files", None, Some(r#"["C:\\a.txt"]"#));
        assert_eq!(extract_merge_text(&i), Some("C:\\a.txt".into()));
    }
}
