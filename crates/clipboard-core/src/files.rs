use crate::{
    HISTORY_LIMIT, History,
    database::{ContentType, NewClipboardItem},
};
use anyhow::{Result, bail};
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
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"files:");
        hasher.update(&serialized);
        let hash = hasher.finalize().to_hex().to_string();
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
        let (_, deleted_images, deleted_payloads) =
            self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        self.cleanup_images(deleted_images, images_dir);
        self.cleanup_staged(deleted_payloads, images_dir);
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
    pub fn files_for_copy(&self, id: i64, staged_dir: &Path) -> Result<Vec<String>> {
        let item = self.item(id)?;
        if item.content_type != "files" {
            bail!("记录不是文件");
        }
        let originals = parse_file_paths(item.file_paths.as_deref())?;
        if originals.iter().all(|path| Path::new(path).exists()) {
            return Ok(originals);
        }
        let root = fs::canonicalize(staged_dir).ok();
        let staged: HashMap<String, String> = item
            .file_payload
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.get("staged")?.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| {
                Some((
                    entry.get("original")?.as_str()?.to_owned(),
                    entry.get("staged")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        Ok(originals
            .into_iter()
            .map(|original| {
                if Path::new(&original).exists() {
                    return original;
                }
                staged
                    .get(&original)
                    .filter(|candidate| {
                        root.as_ref().is_some_and(|root| {
                            fs::canonicalize(candidate)
                                .ok()
                                .is_some_and(|path| path.is_file() && path.parent() == Some(root))
                        })
                    })
                    .cloned()
                    .unwrap_or(original)
            })
            .collect())
    }

    pub(crate) fn cleanup_staged(&self, candidates: Vec<String>, images_dir: &Path) {
        if candidates.is_empty() {
            return;
        }
        let Some(staged_dir) = images_dir.parent().map(|parent| parent.join("staged")) else {
            return;
        };
        let Ok(root) = fs::canonicalize(&staged_dir) else {
            return;
        };
        let Ok(current_payloads) = self.repo.get_all_file_payloads() else {
            return;
        };
        let used: HashSet<PathBuf> = current_payloads
            .iter()
            .flat_map(|payload| staged_paths(payload))
            .filter_map(|path| fs::canonicalize(path).ok())
            .collect();
        for path in candidates.iter().flat_map(|payload| staged_paths(payload)) {
            let raw = Path::new(&path);
            let Ok(canonical) = fs::canonicalize(raw) else {
                continue;
            };
            if raw
                .parent()
                .and_then(|parent| fs::canonicalize(parent).ok())
                == Some(root.clone())
                && canonical.parent() == Some(root.as_path())
                && !used.contains(&canonical)
            {
                let _ = fs::remove_file(raw);
            }
        }
    }
}

fn staged_paths(raw: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| value.get("staged")?.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| entry.get("staged")?.as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PAGE_SIZE, PreviewContent};

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
        let id = history.capture_files(&files, &directory.path().join("images"))?;
        let text = history.capture("text")?.unwrap();
        assert_eq!(
            history.capture_files(&files, &directory.path().join("images"))?,
            id
        );
        assert_eq!(history.files(id)?, files);
        assert_eq!(history.preview_content(id)?, PreviewContent::Files(files));
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
            let resolved = history.files_for_copy(id, &staged_dir)?;
            assert_eq!(
                resolved,
                vec![if candidate == &staged {
                    staged.to_string_lossy().into_owned()
                } else {
                    original.clone()
                }]
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
        assert_eq!(history.clear_history_with_media(None, &images)?, 1);
        assert!(staged.exists());
        history.toggle_pin(ids[0])?;
        assert_eq!(history.clear_history_with_media(None, &images)?, 1);
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
