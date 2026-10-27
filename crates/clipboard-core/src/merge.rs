use crate::{History, MAX_FILE_PATHS, MAX_PATH_LIST_BYTES};
use anyhow::{Context, Result, bail};
use std::{collections::HashSet, path::Path};

#[derive(Debug)]
pub struct MergedContent {
    pub text: Option<String>,
    pub files: Vec<String>,
}

impl History {
    pub fn merge_content(&self, ids: &[i64]) -> Result<MergedContent> {
        if ids.len() < 2 {
            bail!("请至少选择两条记录进行合并");
        }
        let mut seen_ids = HashSet::with_capacity(ids.len());
        if ids.iter().any(|id| !seen_ids.insert(*id)) {
            bail!("合并列表包含重复记录");
        }

        let mut text_parts = Vec::with_capacity(ids.len());
        let mut files = Vec::new();
        let mut seen_files = HashSet::new();
        for id in ids {
            let item = self
                .item(*id)
                .with_context(|| format!("无法读取待合并记录 {id}"))?;
            match item.content_type.as_str() {
                "files" => {
                    let paths = Self::file_paths_for_copy(&item)
                        .with_context(|| format!("无法读取记录 {id} 的文件路径"))?;
                    if paths.iter().any(|path| !Path::new(path).exists()) {
                        bail!("记录 {id} 的源文件或文件夹已不存在，无法合并");
                    }
                    text_parts.push(paths.join("\n"));
                    append_unique_files(&mut files, &mut seen_files, paths);
                }
                "image" => {
                    let path = item.image_path.context("图片文件路径缺失")?;
                    if !Path::new(&path).is_file() {
                        bail!("记录 {id} 的图片文件已丢失，无法合并");
                    }
                    text_parts.push(path.clone());
                    append_unique_files(&mut files, &mut seen_files, [path]);
                }
                "text" | "url" | "html" | "rtf" => {
                    let text = merge_text(&item)
                        .with_context(|| format!("记录 {id} 没有可合并的文本表示"))?;
                    text_parts.push(text);
                }
                kind => bail!("记录 {id} 的类型 {kind} 不支持合并"),
            }
        }

        if files.len() > MAX_FILE_PATHS || serde_json::to_vec(&files)?.len() > MAX_PATH_LIST_BYTES {
            bail!("合并后的文件路径超过 256 项或 1 MiB");
        }

        Ok(MergedContent {
            text: Some(text_parts.join("\n")),
            files,
        })
    }
}

fn append_unique_files(
    target: &mut Vec<String>,
    seen: &mut HashSet<String>,
    paths: impl IntoIterator<Item = String>,
) {
    for path in paths {
        if seen.insert(path.clone()) {
            target.push(path);
        }
    }
}

fn merge_text(item: &crate::database::ClipboardItem) -> Result<String> {
    if let Some(text) = item.text_content.as_ref().filter(|text| !text.is_empty()) {
        return Ok(text.clone());
    }
    if let Some(html) = item.html_content.as_deref() {
        let text = crate::html_text::html_to_text(html)?;
        if !text.is_empty() {
            return Ok(text);
        }
    }
    item.preview
        .clone()
        .filter(|preview| !preview.is_empty() && !preview.starts_with('['))
        .context("记录没有可合并的文本表示")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_in_requested_order_and_deduplicates_files() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let first = history.capture("第一条")?.unwrap();
        let shared = directory.path().join("共享.txt");
        let other = directory.path().join("other.txt");
        std::fs::write(&shared, "shared")?;
        std::fs::write(&other, "other")?;
        let files = history.capture_files(
            &[
                shared.to_string_lossy().into_owned(),
                other.to_string_lossy().into_owned(),
            ],
            &images,
        )?;
        let repeated = history.capture_files(&[shared.to_string_lossy().into_owned()], &images)?;

        let merged = history.merge_content(&[files, first, repeated])?;
        let expected_text = format!(
            "{}\n{}\n第一条\n{}",
            shared.display(),
            other.display(),
            shared.display()
        );
        assert_eq!(merged.text.as_deref(), Some(expected_text.as_str()));
        assert_eq!(
            merged.files,
            vec![
                shared.to_string_lossy().into_owned(),
                other.to_string_lossy().into_owned()
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_selection_and_missing_media() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let text = history.capture("text")?.unwrap();
        assert!(history.merge_content(&[text]).is_err());
        assert!(history.merge_content(&[text, text]).is_err());

        let missing = directory.path().join("missing.txt");
        let file = history.capture_files(&[missing.to_string_lossy().into_owned()], &images)?;
        let error = history.merge_content(&[text, file]).unwrap_err();
        assert!(error.to_string().contains("已不存在"));
        Ok(())
    }

    #[test]
    fn uses_html_fallback_and_rejects_unrepresentable_rtf() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let text = history.capture("plain")?.unwrap();
        let html = history.capture_rich(Some("<p>Hello <b>world</b></p>"), None, None, &images)?;
        let mut legacy = history.item(html)?;
        legacy.text_content = None;
        legacy.html_content = Some("<p>A &amp; B</p><p>C</p><style>hidden</style>".into());
        assert_eq!(merge_text(&legacy)?, "A & B\nC");
        legacy.html_content = Some("\0".repeat(crate::MAX_TEXT_BYTES / 3 + 1));
        assert!(merge_text(&legacy).is_err());
        assert_eq!(
            history.merge_content(&[html, text])?.text.as_deref(),
            Some("Hello world\nplain")
        );

        let rtf = history.capture_rich(None, Some(b"{\\rtf1 no text}"), None, &images)?;
        let error = history.merge_content(&[text, rtf]).unwrap_err();
        assert!(error.to_string().contains("没有可合并的文本表示"));
        Ok(())
    }
}
