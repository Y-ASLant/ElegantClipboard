use crate::image::{canonical_media_path, remove_media_file};
use crate::{
    FilePreviewEntry, History,
    database::{ClipboardItem, ContentType, NewClipboardItem},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::{HashMap, HashSet},
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

impl History {
    pub fn capture_files(&self, paths: &[String], images_dir: &Path) -> Result<i64> {
        if paths.is_empty()
            || paths.len() > MAX_FILE_PATHS
            || paths.iter().any(|path| path.is_empty())
        {
            bail!("文件路径列表为空或超过 256 项");
        }
        let serialized = serde_json::to_vec(paths)?;
        if serialized.len() > MAX_PATH_LIST_BYTES {
            bail!("文件路径列表超过 1 MiB");
        }
        let hash = crate::clipboard::hash_with_prefix(b"files:", &serialized);
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(id);
        }
        let preview = if paths.len() == 1 {
            Path::new(&paths[0])
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| paths[0].clone())
        } else {
            format!("{} 个文件或文件夹", paths.len())
        };
        let id = self.repo.insert(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(paths.to_vec()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            preview: Some(preview.chars().take(240).collect()),
            byte_size: serialized.len() as i64,
            ..Default::default()
        })?;
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

    /// Prefer original files; use a recovered staged copy only when it is
    /// still contained in this GPUI data directory's staged folder.
    pub fn file_paths_for_copy(item: &ClipboardItem, staged_dir: &Path) -> Result<Vec<String>> {
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        Ok(resolved_file_paths(
            parse_file_paths(item.file_paths.as_deref())?,
            item.file_payload.as_deref(),
            staged_dir,
        ))
    }

    pub(crate) fn file_preview_entries(
        item: &ClipboardItem,
        staged_dir: Option<&Path>,
    ) -> Result<Vec<FilePreviewEntry>> {
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        let originals = parse_file_paths(item.file_paths.as_deref())?;
        let resolved = staged_dir.map_or_else(
            || originals.clone(),
            |dir| resolved_file_paths(originals.clone(), item.file_payload.as_deref(), dir),
        );
        Ok(originals
            .into_iter()
            .zip(resolved)
            .map(|(original_path, resolved_path)| {
                let recovered = original_path != resolved_path;
                match fs::metadata(&resolved_path) {
                    Ok(metadata) => FilePreviewEntry {
                        original_path,
                        resolved_path,
                        exists: true,
                        is_dir: metadata.is_dir(),
                        size: metadata.is_file().then_some(metadata.len()),
                        recovered,
                        metadata_error: None,
                    },
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        FilePreviewEntry {
                            original_path,
                            resolved_path,
                            exists: false,
                            is_dir: false,
                            size: None,
                            recovered,
                            metadata_error: None,
                        }
                    }
                    Err(error) => FilePreviewEntry {
                        original_path,
                        resolved_path,
                        exists: false,
                        is_dir: false,
                        size: None,
                        recovered,
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
        let staged_dir = images_dir
            .parent()
            .context("无法确定暂存目录")?
            .join("staged");
        let Some(root) = canonical_media_path(&staged_dir)? else {
            return Ok(());
        };
        let current_payloads = self
            .repo
            .get_all_file_payloads()
            .context("无法读取暂存附件引用，未回收暂存附件")?;
        let mut used = HashSet::<PathBuf>::new();
        for payload in current_payloads {
            for path in staged_paths(&payload)? {
                if let Some(path) = canonical_media_path(Path::new(&path))? {
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
                    let raw = Path::new(&path);
                    let parent = raw.parent().context("暂存附件路径没有父目录")?;
                    if canonical_media_path(parent)?.as_deref() != Some(root.as_path()) {
                        return Ok(());
                    }
                    let Some(canonical) = canonical_media_path(raw)? else {
                        return Ok(());
                    };
                    if canonical.parent() == Some(root.as_path()) && !used.contains(&canonical) {
                        remove_media_file(raw)?;
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

fn resolved_file_paths(
    originals: Vec<String>,
    payload: Option<&str>,
    staged_dir: &Path,
) -> Vec<String> {
    if originals.iter().all(|path| Path::new(path).exists()) {
        return originals;
    }
    let Ok(root) = fs::canonicalize(staged_dir) else {
        return originals;
    };
    let staged: HashMap<String, String> = payload
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| {
            value.get("staged")?.as_array().map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        Some((
                            entry.get("original")?.as_str()?.to_owned(),
                            entry.get("staged")?.as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
        })
        .unwrap_or_default();
    originals
        .into_iter()
        .map(|original| {
            if Path::new(&original).exists() {
                return original;
            }
            staged
                .get(&original)
                .filter(|candidate| {
                    fs::canonicalize(candidate)
                        .ok()
                        .is_some_and(|path| path.is_file() && path.parent() == Some(root.as_path()))
                })
                .cloned()
                .unwrap_or(original)
        })
        .collect()
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
                recovered: false,
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
    fn staged_fallback_accepts_only_files_inside_managed_directory() -> Result<()> {
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
            let resolved = History::file_paths_for_copy(&history.item(id)?, &staged_dir)?;
            assert_eq!(
                resolved,
                vec![if candidate == &staged {
                    staged.to_string_lossy().into_owned()
                } else {
                    original.clone()
                }]
            );
            assert_eq!(
                history.preview_content_with_staged(id, &staged_dir)?,
                PreviewContent::Files(vec![FilePreviewEntry {
                    original_path: original.clone(),
                    resolved_path: if candidate == &staged {
                        staged.to_string_lossy().into_owned()
                    } else {
                        original.clone()
                    },
                    exists: candidate == &staged,
                    is_dir: false,
                    size: (candidate == &staged).then_some(9),
                    recovered: candidate == &staged,
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
