//! 合并粘贴：聚合多条记录的文本/富文本/文件格式后一次性写入剪贴板。

use super::file_clipboard::prepare_files;
use super::format_write::{PreparedClipboard, prepare_item};
use crate::database::ClipboardItem;
use crate::operation_error::{OperationError, OperationErrorCode, readable_resource};
use clipboard_rs::ClipboardContent as RsClipboardContent;
use std::path::Path;

pub fn prepare_merge(
    items: &[ClipboardItem],
    separator: &str,
) -> Result<PreparedClipboard, OperationError> {
    if items.is_empty() {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "no selected items",
        ));
    }
    let mut contents = Vec::new();
    let mut merged_paths = Vec::new();
    let mut text_parts = Vec::new();
    let mut html_parts = Vec::new();
    let mut rtf_decoded = Vec::new();
    let mut extras: Vec<(String, Vec<u8>)> = Vec::new();
    let mut virtual_count = 0;
    for item in items {
        match item.content_type.as_str() {
            "files" => {
                let files =
                    prepare_files(item.file_paths.as_deref(), item.file_payload.as_deref())?;
                if files.paths.is_empty() {
                    virtual_count += 1;
                }
                if !files.paths.is_empty() {
                    text_parts.push(files.paths.join("\n"));
                }
                for path in files.paths {
                    if !merged_paths.contains(&path) {
                        merged_paths.push(path);
                    }
                }
                for (name, bytes) in files.extras {
                    if let Some((_, previous)) =
                        extras.iter().find(|(existing, _)| existing == &name)
                    {
                        if previous != &bytes {
                            return Err(OperationError::new(
                                OperationErrorCode::UnsupportedContent,
                                "incompatible merged file formats",
                            ));
                        }
                    } else {
                        extras.push((name, bytes));
                    }
                }
            }
            "image" => {
                let path = item
                    .image_path
                    .as_deref()
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| {
                        OperationError::new(
                            OperationErrorCode::InvalidContent,
                            "merged image path absent",
                        )
                    })?;
                if !readable_resource(Path::new(path))?.is_file() {
                    return Err(OperationError::new(
                        OperationErrorCode::UnsupportedContent,
                        "merged image source is not a regular file",
                    ));
                }
                if !merged_paths.iter().any(|existing| existing == path) {
                    merged_paths.push(path.to_string());
                }
                text_parts.push(path.to_string());
            }
            _ => {
                if let PreparedClipboard::Contents(prepared) = prepare_item(item)? {
                    for content in prepared {
                        match content {
                            RsClipboardContent::Text(text) => text_parts.push(text),
                            RsClipboardContent::Html(html) => html_parts.push(html),
                            RsClipboardContent::Other(name, bytes)
                                if name == "Rich Text Format" =>
                            {
                                rtf_decoded.push(bytes)
                            }
                            content => contents.push(content),
                        }
                    }
                }
            }
        }
    }
    if virtual_count > 1 || (virtual_count > 0 && !merged_paths.is_empty()) {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "virtual files cannot be combined with other file payloads",
        ));
    }
    if !merged_paths.is_empty() {
        contents.push(RsClipboardContent::Files(merged_paths));
    }
    for (name, bytes) in extras {
        contents.push(RsClipboardContent::Other(name, bytes));
    }
    if !text_parts.is_empty() {
        contents.push(RsClipboardContent::Text(text_parts.join(separator)));
    }

    if !html_parts.is_empty() {
        contents.push(RsClipboardContent::Html(html_parts.join(separator)));
    }

    if rtf_decoded.len() == 1 {
        contents.push(RsClipboardContent::Other(
            "Rich Text Format".to_string(),
            rtf_decoded.into_iter().next().expect("one RTF payload"),
        ));
    }

    if contents.is_empty() {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "selected items have no merge content",
        ));
    }
    Ok(PreparedClipboard::Contents(contents))
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
        }
    }

    #[test]
    fn preparation_rejects_any_empty_or_missing_selected_item() {
        let valid = item(1, "text", Some("hello"), None);
        let empty = item(2, "text", None, None);
        assert!(prepare_merge(&[valid.clone(), empty], "\n").is_err());
        let missing = item(
            3,
            "files",
            None,
            Some(r#"["C:\\__ec_missing_merge__\\absent.txt"]"#),
        );
        assert!(matches!(
            prepare_merge(&[valid, missing], "\n"),
            Err(OperationError {
                code: OperationErrorCode::ResourceMissing,
                ..
            })
        ));
        assert!(matches!(
            prepare_merge(&[item(4, "image", None, None)], "\n"),
            Err(OperationError {
                code: OperationErrorCode::InvalidContent,
                ..
            })
        ));
        assert!(matches!(
            prepare_merge(&[item(5, "unknown", Some("text"), None)], "\n"),
            Err(OperationError {
                code: OperationErrorCode::UnsupportedContent,
                ..
            })
        ));
    }

    #[test]
    fn preparation_joins_text_and_rejects_empty_selection() {
        let PreparedClipboard::Contents(contents) = prepare_merge(
            &[
                item(1, "text", Some("a"), None),
                item(2, "text", Some("b"), None),
            ],
            "|",
        )
        .unwrap() else {
            panic!("expected formats");
        };
        assert!(matches!(&contents[0], RsClipboardContent::Text(text) if text == "a|b"));
        assert!(prepare_merge(&[], "\n").is_err());
    }

    #[test]
    fn image_file_merge_does_not_require_preview_decoding() {
        let path = std::env::temp_dir().join(format!("ec_merge_raw_{}.png", std::process::id()));
        std::fs::write(&path, b"raw bytes, not an image").unwrap();
        let mut image = item(1, "image", None, None);
        image.image_path = Some(path.to_string_lossy().into_owned());
        assert!(prepare_merge(&[image.clone()], "\n").is_ok());
        assert!(matches!(
            prepare_item(&image),
            Err(OperationError {
                code: OperationErrorCode::ImageDecodeFailed,
                ..
            })
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn merged_files_preserve_selection_order_and_deduplicate_paths() {
        let directory = std::env::temp_dir().join(format!("ec_merge_files_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let a = directory.join("a.txt").to_string_lossy().into_owned();
        let b = directory.join("b.txt").to_string_lossy().into_owned();
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        let paths = serde_json::to_string(&vec![a.clone(), b.clone()]).unwrap();
        let duplicate = serde_json::to_string(&vec![a.clone()]).unwrap();
        let PreparedClipboard::Contents(contents) = prepare_merge(
            &[
                item(1, "files", None, Some(&paths)),
                item(2, "files", None, Some(&duplicate)),
            ],
            "\n",
        )
        .unwrap() else {
            panic!("expected formats");
        };
        assert!(matches!(&contents[0], RsClipboardContent::Files(paths) if paths == &vec![a, b]));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_encoded_rtf_is_not_silently_dropped_for_text_fallback() {
        let mut rich = item(1, "rtf", Some("plain fallback"), None);
        rich.rtf_content = Some("b64:not valid base64!".into());
        assert!(matches!(
            prepare_item(&rich),
            Err(OperationError {
                code: OperationErrorCode::InvalidContent,
                ..
            })
        ));
        assert!(prepare_merge(&[rich], "\n").is_err());
    }

    #[test]
    fn cached_preview_alone_is_not_source_content() {
        let mut rich = item(1, "html", None, None);
        rich.preview = Some("stale cached preview".into());
        assert!(matches!(
            prepare_item(&rich),
            Err(OperationError {
                code: OperationErrorCode::InvalidContent,
                ..
            })
        ));
    }
}
