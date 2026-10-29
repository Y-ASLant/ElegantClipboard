use crate::{History, database::ContentType};
use anyhow::{Result, anyhow};
use std::path::PathBuf;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreviewEntry {
    pub original_path: String,
    pub resolved_path: String,
    pub exists: bool,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub metadata_error: Option<String>,
}

/// 完整预览与悬停预览的内容载荷。worker 线程构造后经事件通道移动到 UI 线程。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewContent {
    Text(String),
    Image(PathBuf),
    Files(Vec<FilePreviewEntry>),
    /// 富文本条目的纯文本表示（RTF 或缺 HTML 原文的旧数据）。
    RichText(String),
    /// HTML 原文（优先取 StartFragment 片段），由 gpui-kit TextView 渲染。
    Html(String),
}

/// ASCII 大小写不敏感的子串查找（无分配；按字节扫描，命中点必在 needle 边界，
/// 对 UTF-8 haystack 需额外验证命中处是字符边界才可信）。
fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&offset| {
        haystack.is_char_boundary(offset)
            && haystack.is_char_boundary(offset + needle.len())
            && haystack[offset..offset + needle.len()].eq_ignore_ascii_case(needle)
    })
}

/// Windows 剪贴板 HTML Format 的片段标记（大小写不敏感处理）。
pub fn html_preview_fragment(html: &str) -> String {
    const START: &str = "<!--startfragment-->";
    const END: &str = "<!--endfragment-->";
    let Some(start) = find_ignore_ascii_case(html, START) else {
        return html.to_owned();
    };
    let content_start = start + START.len();
    let Some(end) = find_ignore_ascii_case(&html[content_start..], END) else {
        return html.to_owned();
    };
    let fragment = &html[content_start..content_start + end];
    if fragment.trim().is_empty() {
        html.to_owned()
    } else {
        fragment.to_owned()
    }
}

impl History {
    pub fn preview_content(&self, id: i64) -> Result<PreviewContent> {
        let item = self.item(id)?;
        if item.content_type == "files" {
            return Ok(PreviewContent::Files(History::file_preview_entries(&item)?));
        }
        if ContentType::parse(&item.content_type).is_some_and(ContentType::is_rich) {
            if item.content_type == "html"
                && let Some(html) = item
                    .html_content
                    .as_deref()
                    .filter(|value| !value.is_empty())
            {
                return Ok(PreviewContent::Html(html_preview_fragment(html)));
            }
            return Ok(PreviewContent::RichText(
                item.text_content
                    .or(item.preview)
                    .unwrap_or_else(|| "[富文本内容，无纯文本预览]".into()),
            ));
        }
        if item.content_type == "image" {
            let path = PathBuf::from(item.image_path.ok_or_else(|| anyhow!("图片路径缺失"))?);
            if !path.is_file() {
                anyhow::bail!("图片文件已丢失，无法预览");
            }
            Ok(PreviewContent::Image(path))
        } else {
            Ok(PreviewContent::Text(
                item.text_content
                    .ok_or_else(|| anyhow!("记录没有可预览的文本"))?,
            ))
        }
    }
}
