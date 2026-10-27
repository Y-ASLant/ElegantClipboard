use crate::file_copy_cleanup::{checked_staged_directory, is_reparse};
use crate::{
    FilePreviewEntry, History,
    database::{ClipboardItem, ContentType, NewClipboardItem},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub const MAX_FILE_PATHS: usize = 256;
pub const MAX_PATH_LIST_BYTES: usize = 1024 * 1024;

pub(crate) fn parse_file_paths(raw: Option<&str>) -> Result<Vec<String>> {
    let paths: Vec<String> =
        serde_json::from_str(raw.ok_or_else(|| anyhow::anyhow!("文件路径缺失"))?)?;
    if paths.is_empty() || paths.len() > MAX_FILE_PATHS || paths.iter().any(|path| path.is_empty())
    {
        bail!("文件路径列表无效");
    }
    Ok(paths)
}

pub(crate) fn file_capture_item(paths: &[String]) -> Result<NewClipboardItem> {
    if paths.is_empty() || paths.len() > MAX_FILE_PATHS || paths.iter().any(|path| path.is_empty())
    {
        bail!("文件路径列表为空或超过 256 项");
    }
    let serialized = serde_json::to_vec(paths)?;
    if serialized.len() > MAX_PATH_LIST_BYTES {
        bail!("文件路径列表超过 1 MiB");
    }
    let hash = crate::clipboard::hash_with_prefix(b"files:", &serialized);
    let preview = if paths.len() == 1 {
        Path::new(&paths[0])
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| paths[0].clone())
    } else {
        format!("{} 个文件或文件夹", paths.len())
    };
    Ok(NewClipboardItem {
        content_type: ContentType::Files,
        file_paths: Some(paths.to_vec()),
        content_hash: hash.clone(),
        semantic_hash: hash,
        preview: Some(preview.chars().take(240).collect()),
        byte_size: serialized.len() as i64,
        ..Default::default()
    })
}

impl History {
    pub fn capture_files(&self, paths: &[String], images_dir: &Path) -> Result<i64> {
        let item = file_capture_item(paths)?;
        if self.dedup_allows_reuse()
            && let Some(id) = self.repo.touch_by_hash(&item.content_hash, None)?
        {
            return Ok(id);
        }
        let id = self.repo.insert(item)?;
        self.enforce_history_limit(Some(images_dir))?;
        Ok(id)
    }

    pub fn files(&self, id: i64) -> Result<Vec<String>> {
        let item = self.item(id)?;
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        parse_file_paths(item.file_paths.as_deref())
    }

    /// 普通文件历史始终仅使用原始路径进行复制。
    pub fn file_paths_for_copy(item: &ClipboardItem) -> Result<Vec<String>> {
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        parse_file_paths(item.file_paths.as_deref())
    }

    pub(crate) fn file_preview_entries(item: &ClipboardItem) -> Result<Vec<FilePreviewEntry>> {
        Ok(Self::file_paths_for_copy(item)?
            .into_iter()
            .map(|original_path| {
                let resolved_path = original_path.clone();
                match fs::metadata(&resolved_path) {
                    Ok(metadata) => FilePreviewEntry {
                        original_path,
                        resolved_path,
                        exists: true,
                        is_dir: metadata.is_dir(),
                        size: metadata.is_file().then_some(metadata.len()),
                        metadata_error: None,
                    },
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        FilePreviewEntry {
                            original_path,
                            resolved_path,
                            exists: false,
                            is_dir: false,
                            size: None,
                            metadata_error: None,
                        }
                    }
                    Err(error) => FilePreviewEntry {
                        original_path,
                        resolved_path,
                        exists: false,
                        is_dir: false,
                        size: None,
                        metadata_error: Some(error.to_string()),
                    },
                }
            })
            .collect())
    }

    pub(crate) fn cleanup_staged(&self, candidates: Vec<String>, images_dir: &Path) -> Result<()> {
        if candidates.is_empty() {
            return Ok(());
        }
        let data_dir = images_dir.parent().context("无法确定暂存目录")?;
        let Some(root) = checked_staged_directory(data_dir)? else {
            return Ok(());
        };
        let staged_dir = std::path::absolute(data_dir.join("staged"))?;
        let current_payloads = self
            .repo
            .get_all_file_payloads()
            .context("无法读取暂存附件引用，未回收暂存附件")?;
        let mut used = HashSet::<PathBuf>::new();
        for payload in current_payloads {
            for path in staged_paths(&payload)? {
                if let Some(path) = legacy_staged_file(Path::new(&path), &staged_dir, &root)? {
                    used.insert(path);
                }
            }
        }
        let mut errors = Vec::new();
        for payload in candidates {
            let paths = match staged_paths(&payload) {
                Ok(paths) => paths,
                Err(error) => {
                    errors.push(format!("{error:#}"));
                    continue;
                }
            };
            for path in paths {
                let result = (|| -> Result<()> {
                    if let Some(file) = legacy_staged_file(Path::new(&path), &staged_dir, &root)?
                        && !used.contains(&file)
                    {
                        fs::remove_file(&file)
                            .with_context(|| format!("无法清理旧文件副本 {}", file.display()))?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    errors.push(format!("{error:#}"));
                }
            }
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("; "));
        }
        Ok(())
    }
}

fn legacy_staged_file(path: &Path, staged_dir: &Path, root: &Path) -> Result<Option<PathBuf>> {
    // 不检查载荷中指向本应用暂存目录以外的路径。
    let Ok(relative) = path
        .strip_prefix(staged_dir)
        .or_else(|_| path.strip_prefix(root))
    else {
        return Ok(None);
    };
    if !relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Ok(None);
    }
    let mut current = root.to_owned();
    for component in relative.components() {
        current.push(component);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("无法检查旧文件副本路径"),
        };
        if is_reparse(&metadata) {
            return Ok(None);
        }
    }
    anyhow::ensure!(
        current != root && fs::symlink_metadata(&current)?.is_file(),
        "旧文件副本不是普通文件：{}",
        current.display()
    );
    Ok(Some(current))
}

fn staged_paths(raw: &str) -> Result<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(raw).context("暂存附件载荷无效")?;
    let Some(staged) = value.get("staged") else {
        return Ok(Vec::new());
    };
    staged
        .as_array()
        .context("暂存附件列表无效")?
        .iter()
        .map(|entry| {
            entry
                .get("staged")
                .and_then(serde_json::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .context("暂存附件路径无效")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilePreviewEntry, PAGE_SIZE, PreviewContent};

    #[test]
    fn file_paths_are_saved_deduplicated_and_reordered_with_text() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let files = vec![
            directory
                .path()
                .join("a.txt")
                .to_string_lossy()
                .into_owned(),
        ];
        fs::write(&files[0], b"sample")?;
        let id = history.capture_files(&files, &directory.path().join("images"))?;
        let text = history.capture("text")?.unwrap();
        assert_eq!(
            history.capture_files(&files, &directory.path().join("images"))?,
            id
        );
        assert_eq!(history.files(id)?, files);
        assert_eq!(
            history.preview_content(id)?,
            PreviewContent::Files(vec![FilePreviewEntry {
                original_path: files[0].clone(),
                resolved_path: files[0].clone(),
                exists: true,
                is_dir: false,
                size: Some(6),
                metadata_error: None,
            }])
        );
        history.reorder(text, id, false, false)?;
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, text);
        assert_eq!(history.count("", false)?, 2);
        Ok(())
    }

    #[test]
    fn rejects_empty_and_oversized_path_lists() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        assert!(history.capture_files(&[], &images).is_err());
        assert!(
            history
                .capture_files(&vec!["x".into(); 257], &images)
                .is_err()
        );
        assert!(history.capture_files(&["".into()], &images).is_err());
        assert!(
            history
                .capture_files(&["x".repeat(MAX_PATH_LIST_BYTES)], &images)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn copy_and_preview_keep_missing_originals_even_with_legacy_copies() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let staged_dir = directory.path().join("staged");
        fs::create_dir_all(&staged_dir)?;
        let staged = staged_dir.join("recovered.txt");
        fs::write(&staged, b"recovered")?;
        let outside = directory.path().join("outside.txt");
        fs::write(&outside, b"outside")?;
        let original = "Z:\\ElegantClipboard-QA-missing\\file.txt".to_owned();
        let id = history.capture_files(
            std::slice::from_ref(&original),
            &directory.path().join("images"),
        )?;
        for candidate in [&outside, &staged] {
            let payload = serde_json::json!({
                "staged": [{"original": original, "staged": candidate.to_string_lossy()}]
            });
            history.db.write_connection().lock().execute(
                "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
                rusqlite::params![payload.to_string(), id],
            )?;
            assert_eq!(
                History::file_paths_for_copy(&history.item(id)?)?,
                vec![original.clone()]
            );
            assert_eq!(
                history.preview_content(id)?,
                PreviewContent::Files(vec![FilePreviewEntry {
                    original_path: original.clone(),
                    resolved_path: original.clone(),
                    exists: false,
                    is_dir: false,
                    size: None,
                    metadata_error: None,
                }])
            );
        }
        history.delete_with_media(id, &directory.path().join("images"))?;
        assert!(!staged.exists());
        assert!(outside.exists());
        Ok(())
    }

    #[test]
    fn clearing_history_preserves_shared_staged_file_until_last_reference() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let staged_dir = directory.path().join("staged");
        fs::create_dir_all(&staged_dir)?;
        let staged = staged_dir.join("shared.txt");
        fs::write(&staged, b"shared")?;
        let mut ids = Vec::new();
        for suffix in ["a", "b"] {
            let original = format!("Z:\\ElegantClipboard-QA-missing\\{suffix}.txt");
            let id = history.capture_files(std::slice::from_ref(&original), &images)?;
            let payload = serde_json::json!({
                "staged": [{"original": original, "staged": staged.to_string_lossy()}]
            });
            history.db.write_connection().lock().execute(
                "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
                rusqlite::params![payload.to_string(), id],
            )?;
            ids.push(id);
        }
        history.toggle_pin(ids[0])?;
        assert_eq!(history.clear_history_with_media(None, &images)?.deleted, 1);
        assert!(staged.exists());
        history.toggle_pin(ids[0])?;
        assert_eq!(history.clear_history_with_media(None, &images)?.deleted, 1);
        assert!(!staged.exists());
        Ok(())
    }

    #[test]
    fn text_eviction_removes_unreferenced_staged_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let staged_dir = directory.path().join("staged");
        fs::create_dir_all(&staged_dir)?;
        let staged = staged_dir.join("old.txt");
        fs::write(&staged, b"old")?;
        let original = "Z:\\ElegantClipboard-QA-missing\\old.txt".to_owned();
        let id = history.capture_files(std::slice::from_ref(&original), &images)?;
        let payload = serde_json::json!({
            "staged": [{"original": original, "staged": staged.to_string_lossy()}]
        });
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            rusqlite::params![payload.to_string(), id],
        )?;
        history.db.write_connection().lock().execute_batch(
            "WITH RECURSIVE nums(n) AS (
               SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 9999
             )
             INSERT INTO clipboard_items
               (content_type, text_content, content_hash, semantic_hash, created_at)
             SELECT 'text', printf('filler %d', n), printf('filler-hash-%d', n),
                    printf('filler-hash-%d', n), '9999-12-31' FROM nums;",
        )?;
        history.capture_with_media("new text", &images)?;
        assert!(history.item(id).is_err());
        assert!(!staged.exists());
        Ok(())
    }
}
