//! 将数据库条目写回系统剪贴板（尽量保留原始格式）

use crate::database::ClipboardItem;
use crate::operation_error::{OperationError, OperationErrorCode, readable_resource};
use clipboard_rs::common::RustImage;
use clipboard_rs::{
    Clipboard as ClipboardTrait, ClipboardContent as RsClipboardContent, ClipboardContext,
};
use std::path::Path;

pub enum PreparedClipboard {
    Contents(Vec<RsClipboardContent>),
    Files(super::file_clipboard::PreparedFiles),
}

impl PreparedClipboard {
    pub fn write(self, ctx: &mut ClipboardContext) -> Result<(), OperationError> {
        match self {
            Self::Contents(contents) => ctx
                .set(contents)
                .map_err(|error| OperationError::clipboard("write clipboard", error)),
            Self::Files(files) => files.write(ctx),
        }
    }
}

pub fn prepare_text(text: String) -> Result<PreparedClipboard, OperationError> {
    if text.is_empty() {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "empty text",
        ));
    }
    Ok(PreparedClipboard::Contents(vec![RsClipboardContent::Text(
        text,
    )]))
}

pub fn prepare_item(item: &ClipboardItem) -> Result<PreparedClipboard, OperationError> {
    match item.content_type.as_str() {
        "text" | "url" => prepare_text(item.text_content.clone().unwrap_or_default()),
        "html" | "rtf" => {
            if !item
                .text_content
                .as_deref()
                .is_some_and(|text| !text.is_empty())
                && !item
                    .html_content
                    .as_deref()
                    .is_some_and(|html| !html.is_empty())
                && !item
                    .rtf_content
                    .as_deref()
                    .is_some_and(|rtf| !rtf.is_empty())
            {
                return Err(OperationError::new(
                    OperationErrorCode::InvalidContent,
                    "item has no source rich or text content",
                ));
            }
            let contents = build_rich_contents(item)?;
            if contents.is_empty() {
                return Err(OperationError::new(
                    OperationErrorCode::InvalidContent,
                    "item has no rich or text content",
                ));
            }
            Ok(PreparedClipboard::Contents(contents))
        }
        "image" => {
            let path = item
                .image_path
                .as_deref()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| {
                    OperationError::new(OperationErrorCode::InvalidContent, "image path absent")
                })?;
            if !readable_resource(Path::new(path))?.is_file() {
                return Err(OperationError::new(
                    OperationErrorCode::UnsupportedContent,
                    "image source is not a regular file",
                ));
            }
            let image = image::open(path).map_err(|error| match error {
                image::ImageError::IoError(error) => {
                    OperationError::io("read image", error, OperationErrorCode::ResourceUnreadable)
                }
                error => OperationError::new(
                    OperationErrorCode::ImageDecodeFailed,
                    format!("decode image: {error}"),
                ),
            })?;
            let image = clipboard_rs::RustImageData::from_dynamic_image(image);
            Ok(PreparedClipboard::Contents(vec![
                RsClipboardContent::Image(image),
            ]))
        }
        "files" => super::file_clipboard::prepare_files(
            item.file_paths.as_deref(),
            item.file_payload.as_deref(),
        )
        .map(PreparedClipboard::Files),
        other => Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            format!("unsupported item type: {other}"),
        )),
    }
}

fn build_rich_contents(item: &ClipboardItem) -> Result<Vec<RsClipboardContent>, OperationError> {
    let mut contents: Vec<RsClipboardContent> = Vec::new();

    if let Some(text) = item_alt_text(item) {
        contents.push(RsClipboardContent::Text(text));
    }

    if let Some(html) = item.html_content.as_deref().filter(|h| !h.is_empty()) {
        // clipboard-rs v0.3.4 的 set() 会调用 plain_html_to_cf_html 包装 CF-HTML
        contents.push(RsClipboardContent::Html(html.to_string()));
    }

    if let Some(rtf) = item
        .rtf_content
        .as_deref()
        .filter(|r| super::rtf_storage::should_write_rtf(Some(r)))
    {
        let raw = super::rtf_storage::decode_rtf_for_clipboard(rtf);
        if raw.len() <= 1 {
            return Err(OperationError::new(
                OperationErrorCode::InvalidContent,
                "invalid or empty encoded RTF",
            ));
        }
        contents.push(RsClipboardContent::Other(
            "Rich Text Format".to_string(),
            raw,
        ));
    }

    Ok(contents)
}

/// 提取条目可用的纯文本 fallback（HTML/RTF 写剪贴板时的 Unicode 伴生格式）
pub(crate) fn item_alt_text(item: &ClipboardItem) -> Option<String> {
    item.text_content
        .as_ref()
        .filter(|text| !text.is_empty())
        .cloned()
        .or_else(|| {
            item.html_content
                .as_ref()
                .map(|h| strip_html_tags(h))
                .filter(|t| !t.is_empty())
        })
        .or_else(|| {
            item.preview
                .as_ref()
                .filter(|preview| !preview.is_empty() && !preview.starts_with('['))
                .cloned()
        })
}

/// 提取用于「纯文本粘贴」的字符串
pub fn item_plain_text(item: &ClipboardItem) -> Result<String, OperationError> {
    match item.content_type.as_str() {
        "text" | "url" => item
            .text_content
            .as_ref()
            .filter(|text| !text.is_empty())
            .cloned()
            .ok_or_else(|| {
                OperationError::new(OperationErrorCode::InvalidContent, "item has no plain text")
            }),
        "html" | "rtf" => item_alt_text(item).ok_or_else(|| {
            OperationError::new(OperationErrorCode::InvalidContent, "item has no plain text")
        }),
        other => Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            format!("item type {other} has no plain text representation"),
        )),
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

    #[test]
    fn direct_text_preparation_preserves_whitespace_but_rejects_empty() {
        assert!(matches!(
            prepare_text(String::new()),
            Err(OperationError {
                code: OperationErrorCode::InvalidContent,
                ..
            })
        ));
        let PreparedClipboard::Contents(contents) = prepare_text(" \n ".into()).unwrap() else {
            panic!("expected text");
        };
        assert!(matches!(&contents[0], RsClipboardContent::Text(text) if text == " \n "));
    }
}
