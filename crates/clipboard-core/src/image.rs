use crate::{
    HISTORY_LIMIT, History,
    database::{ContentType, NewClipboardItem},
};
use anyhow::{Context, Result, bail};
use rusqlite::OptionalExtension;
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
};

pub const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 25_000_000;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

impl History {
    pub fn capture_image(
        &self,
        png: &[u8],
        width: u32,
        height: u32,
        images_dir: &Path,
    ) -> Result<i64> {
        if !png.starts_with(PNG_SIGNATURE) || png.len() > MAX_IMAGE_BYTES {
            bail!("图片不是有效 PNG，或超过 50 MiB");
        }
        if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
            bail!("图片尺寸无效或超过 2500 万像素");
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"image:");
        hasher.update(png);
        let hash = hasher.finalize().to_hex().to_string();
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            let item = self.repo.get_by_id(id)?.context("图片记录已不存在")?;
            if item
                .image_path
                .as_deref()
                .is_some_and(|path| Path::new(path).is_file())
            {
                return Ok(id);
            }
            let saved = save_png(png, &hash, images_dir)?;
            let image_path = saved.path.to_str().context("图片路径无法转换为 UTF-8")?;
            let result = self.repo.update_item_media_paths(
                id,
                Some(image_path),
                item.file_payload.as_deref(),
                item.source_app_icon.as_deref(),
            );
            if result.is_err() && saved.created {
                let _ = std::fs::remove_file(&saved.path);
            }
            result?;
            return Ok(id);
        }
        let saved = save_png(png, &hash, images_dir)?;
        let image_path = saved.path.to_str().context("图片路径无法转换为 UTF-8")?;
        let inserted = self.repo.insert(NewClipboardItem {
            content_type: ContentType::Image,
            image_path: Some(image_path.to_owned()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            preview: Some(format!("[图片] {width} × {height}")),
            byte_size: png.len() as i64,
            image_width: Some(i64::from(width)),
            image_height: Some(i64::from(height)),
            ..Default::default()
        });
        if inserted.is_err() && saved.created {
            let _ = std::fs::remove_file(&saved.path);
        }
        let id = inserted?;
        let (_, deleted_images, deleted_payloads) =
            self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        self.cleanup_images(deleted_images, images_dir);
        self.cleanup_staged(deleted_payloads, images_dir);
        Ok(id)
    }

    pub fn delete_with_media(&self, id: i64, images_dir: &Path) -> Result<()> {
        let item = self.repo.get_by_id(id)?;
        let image_path = item.as_ref().and_then(|item| item.image_path.clone());
        let file_payload = item.and_then(|item| item.file_payload);
        self.repo.delete(id)?;
        if let Some(image_path) = image_path {
            self.cleanup_images(vec![image_path], images_dir);
        }
        if let Some(file_payload) = file_payload {
            self.cleanup_staged(vec![file_payload], images_dir);
        }
        Ok(())
    }

    pub fn delete_batch_with_media(
        &self,
        ids: &[i64],
        group_id: Option<i64>,
        images_dir: &Path,
    ) -> Result<i64> {
        if ids.is_empty() || ids.len() > HISTORY_LIMIT as usize {
            bail!("请选择有效数量的记录");
        }
        let mut seen = HashSet::new();
        if ids.iter().any(|id| *id <= 0 || !seen.insert(*id)) {
            bail!("选择的记录 ID 无效或重复");
        }
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let mut images = Vec::new();
        let mut payloads = Vec::new();
        {
            let mut query = tx.prepare(
                "SELECT group_id, image_path, file_payload FROM clipboard_items WHERE id = ?1",
            )?;
            for id in ids {
                let row: Option<(Option<i64>, Option<String>, Option<String>)> = query
                    .query_row([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                    .optional()?;
                let Some((actual_group, image, payload)) = row else {
                    bail!("所选记录已不存在，请刷新列表");
                };
                if actual_group != group_id {
                    bail!("所选记录已不属于当前分组，请刷新列表");
                }
                images.extend(image);
                payloads.extend(payload);
            }
        }
        {
            let mut delete = tx.prepare("DELETE FROM clipboard_items WHERE id = ?1")?;
            for id in ids {
                if delete.execute([id])? != 1 {
                    bail!("所选记录已变化，请重试");
                }
            }
        }
        tx.commit()?;
        drop(connection);
        self.cleanup_images(images, images_dir);
        self.cleanup_staged(payloads, images_dir);
        Ok(ids.len() as i64)
    }

    /// Clear non-pinned, non-favorite records in one group and remove only
    /// unreferenced images owned by this data directory.
    pub fn clear_history_with_media(
        &self,
        group_id: Option<i64>,
        images_dir: &Path,
    ) -> Result<i64> {
        let candidates = self.repo.get_clearable_image_paths(group_id, None)?;
        let staged = self.repo.get_clearable_file_payloads(group_id, None)?;
        let deleted = self.repo.clear_history(group_id, None)?;
        self.cleanup_images(candidates, images_dir);
        self.cleanup_staged(staged, images_dir);
        Ok(deleted)
    }

    /// Delete every history record, including pinned and favorite items,
    /// while preserving groups and settings.
    pub fn clear_all_with_media(&self, images_dir: &Path) -> Result<i64> {
        let images = self.repo.get_all_image_paths()?;
        let payloads = self.repo.get_all_file_payloads()?;
        let deleted = self.repo.clear_all()?;
        self.cleanup_images(images, images_dir);
        self.cleanup_staged(payloads, images_dir);
        Ok(deleted)
    }

    pub(crate) fn cleanup_images(&self, candidates: Vec<String>, images_dir: &Path) {
        if candidates.is_empty() {
            return;
        }
        let Ok(root) = std::fs::canonicalize(images_dir) else {
            return;
        };
        let Ok(used) = self.repo.get_all_image_paths() else {
            return;
        };
        let used: HashSet<String> = used.into_iter().collect();
        for candidate in candidates {
            if used.contains(&candidate) {
                continue;
            }
            let path = Path::new(&candidate);
            if path
                .parent()
                .and_then(|parent| std::fs::canonicalize(parent).ok())
                == Some(root.clone())
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

struct SavedImage {
    path: PathBuf,
    created: bool,
}

fn save_png(png: &[u8], hash: &str, images_dir: &Path) -> Result<SavedImage> {
    std::fs::create_dir_all(images_dir).context("无法创建图片目录")?;
    let images_dir = std::fs::canonicalize(images_dir)?;
    let path = images_dir.join(format!("{hash}.png"));
    if path.is_file() {
        return Ok(SavedImage {
            path,
            created: false,
        });
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&images_dir)?;
    temporary.write_all(png)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&path) {
        Ok(_) => Ok(SavedImage {
            path,
            created: true,
        }),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(SavedImage {
            path,
            created: false,
        }),
        Err(error) => Err(error.error).context("无法保存图片"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use rusqlite::params;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nsynthetic";

    #[test]
    fn image_is_saved_deduplicated_and_missing_file_is_repaired() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        let id = history.capture_image(PNG, 3, 2, &images)?;
        let item = history.repo.get_by_id(id)?.unwrap();
        let image_path = std::path::PathBuf::from(item.image_path.unwrap());
        assert_eq!(std::fs::read(&image_path)?, PNG);
        assert_eq!(item.image_width, Some(3));
        assert_eq!(item.image_height, Some(2));
        assert_eq!(history.capture_image(PNG, 3, 2, &images)?, id);
        assert_eq!(history.repo.count(Default::default())?, 1);
        std::fs::remove_file(&image_path)?;
        assert!(history.preview_content(id).is_err());
        assert_eq!(history.capture_image(PNG, 3, 2, &images)?, id);
        assert_eq!(std::fs::read(&image_path)?, PNG);
        assert_eq!(
            history.preview_content(id)?,
            crate::PreviewContent::Image(image_path)
        );
        Ok(())
    }

    #[test]
    fn rejects_oversize_and_invalid_image_without_writing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        assert!(history.capture_image(b"not-png", 1, 1, &images).is_err());
        assert!(history.capture_image(PNG, 0, 1, &images).is_err());
        assert!(history.capture_image(PNG, 5001, 5000, &images).is_err());
        assert!(
            history
                .capture_image(&vec![0; MAX_IMAGE_BYTES + 1], 1, 1, &images)
                .is_err()
        );
        assert!(!images.exists());
        Ok(())
    }

    #[test]
    fn delete_removes_only_unreferenced_managed_images() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        let id = history.capture_image(PNG, 3, 2, &images)?;
        let managed = PathBuf::from(history.repo.get_by_id(id)?.unwrap().image_path.unwrap());
        let external = directory.path().join("legacy.png");
        std::fs::write(&external, PNG)?;
        let legacy_id = history.repo.insert(NewClipboardItem {
            content_type: ContentType::Image,
            image_path: Some(external.to_str().unwrap().into()),
            content_hash: "legacy-image".into(),
            semantic_hash: "legacy-image".into(),
            ..Default::default()
        })?;
        history.delete_with_media(legacy_id, &images)?;
        assert!(external.exists());
        history.delete_with_media(id, &images)?;
        assert!(!managed.exists());
        Ok(())
    }

    #[test]
    fn clear_history_is_group_scoped_and_preserves_protected_items() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let group = history.create_group("工作")?;
        let group_image = history.capture_image(PNG, 3, 2, &images)?;
        let group_image_path = PathBuf::from(history.item(group_image)?.image_path.unwrap());
        history.move_to_group(group_image, None, Some(group.id))?;
        let group_text = history.capture("remove in group")?.unwrap();
        history.move_to_group(group_text, None, Some(group.id))?;
        let favorite = history.capture("keep favorite")?.unwrap();
        history.move_to_group(favorite, None, Some(group.id))?;
        history.toggle_favorite(favorite)?;
        let pinned = history.capture("keep pinned")?.unwrap();
        history.move_to_group(pinned, None, Some(group.id))?;
        history.toggle_pin(pinned)?;
        let default_text = history.capture("keep default")?.unwrap();

        assert_eq!(
            history.clear_history_with_media(Some(group.id), &images)?,
            2
        );
        assert!(history.item(group_image).is_err());
        assert!(history.item(group_text).is_err());
        assert!(!group_image_path.exists());
        assert!(history.item(favorite)?.is_favorite);
        assert!(history.item(pinned)?.is_pinned);
        assert_eq!(history.text(default_text)?, "keep default");
        assert_eq!(history.clear_history_with_media(None, &images)?, 1);
        assert!(history.item(default_text).is_err());
        assert_eq!(history.groups()?[0].item_count, 2);
        Ok(())
    }

    #[test]
    fn clear_all_removes_protected_records_and_media_but_keeps_groups_and_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&database);
        let images = directory.path().join("images");
        let staged_dir = directory.path().join("staged");
        std::fs::create_dir_all(&staged_dir)?;

        let group = history.create_group("Keep group")?;
        let protected = history.capture("protected")?.unwrap();
        history.toggle_pin(protected)?;
        history.toggle_favorite(protected)?;
        let grouped = history.capture("grouped")?.unwrap();
        history.move_to_group(grouped, None, Some(group.id))?;
        let image = history.capture_image(PNG, 2, 2, &images)?;
        let image_path = PathBuf::from(history.item(image)?.image_path.unwrap());
        let missing = "Z:\\ElegantClipboard-QA-missing\\clear-all.txt".to_owned();
        let file = history.capture_files(std::slice::from_ref(&missing), &images)?;
        let staged = staged_dir.join("clear-all.txt");
        std::fs::write(&staged, b"staged")?;
        let payload = serde_json::json!({
            "staged": [{"original": missing, "staged": staged.to_string_lossy()}]
        });
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            params![payload.to_string(), file],
        )?;
        crate::preferences::Preferences::new(&database)
            .set_theme(crate::preferences::ThemePreference::Dark)?;

        assert_eq!(history.clear_all_with_media(&images)?, 4);

        assert_eq!(history.count("", false)?, 0);
        assert_eq!(history.count_in_group("", false, Some(group.id))?, 0);
        assert!(history.groups()?.iter().any(|item| item.id == group.id));
        assert_eq!(
            crate::preferences::Preferences::new(&database).theme()?,
            crate::preferences::ThemePreference::Dark
        );
        assert!(!image_path.exists());
        assert!(!staged.exists());
        Ok(())
    }

    #[test]
    fn clear_history_stops_before_deleting_rows_with_unreadable_media_paths() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        let id = history.capture_image(PNG, 3, 2, &images)?;
        let path = PathBuf::from(history.item(id)?.image_path.unwrap());
        db.write_connection().lock().execute(
            "UPDATE clipboard_items SET image_path = X'01' WHERE id = ?1",
            params![id],
        )?;

        assert!(history.clear_history_with_media(None, &images).is_err());
        assert_eq!(history.repo.count(Default::default())?, 1);
        assert!(path.exists());
        Ok(())
    }

    #[test]
    fn batch_delete_is_atomic_and_cleans_only_selected_media() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let group = history.create_group("批量测试")?;
        let image = history.capture_image(PNG, 3, 2, &images)?;
        let image_path = PathBuf::from(history.item(image)?.image_path.unwrap());
        history.move_to_group(image, None, Some(group.id))?;
        history.toggle_pin(image)?;

        let staged_dir = directory.path().join("staged");
        std::fs::create_dir_all(&staged_dir)?;
        let staged = staged_dir.join("draft.txt");
        std::fs::write(&staged, b"draft")?;
        let original = "Z:\\missing\\draft.txt".to_owned();
        let file = history.capture_files(std::slice::from_ref(&original), &images)?;
        history.move_to_group(file, None, Some(group.id))?;
        history.toggle_favorite(file)?;
        let payload = serde_json::json!({
            "staged": [{"original": original, "staged": staged.to_string_lossy()}]
        });
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            params![payload.to_string(), file],
        )?;
        let other = history.capture("keep in default")?.unwrap();

        assert!(
            history
                .delete_batch_with_media(&[image, other], Some(group.id), &images)
                .is_err()
        );
        assert!(
            history
                .delete_batch_with_media(&[image, image], Some(group.id), &images)
                .is_err()
        );
        history
            .db
            .write_connection()
            .lock()
            .execute_batch(&format!(
                "CREATE TRIGGER reject_batch BEFORE DELETE ON clipboard_items
             WHEN OLD.id = {file} BEGIN SELECT RAISE(ABORT, 'test failure'); END;"
            ))?;
        assert!(
            history
                .delete_batch_with_media(&[image, file], Some(group.id), &images)
                .is_err()
        );
        assert!(history.item(image).is_ok());
        assert!(history.item(file).is_ok());
        assert!(image_path.exists());
        assert!(staged.exists());
        history
            .db
            .write_connection()
            .lock()
            .execute_batch("DROP TRIGGER reject_batch")?;

        assert_eq!(
            history.delete_batch_with_media(&[image, file], Some(group.id), &images)?,
            2
        );
        assert!(history.item(image).is_err());
        assert!(history.item(file).is_err());
        assert!(!image_path.exists());
        assert!(!staged.exists());
        assert_eq!(history.text(other)?, "keep in default");
        Ok(())
    }

    #[test]
    fn image_and_text_share_the_same_reorder_flow() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let image = history.capture_image(PNG, 3, 2, &directory.path().join("images"))?;
        let text = history.capture("text")?.unwrap();
        assert_eq!(history.list("", crate::PAGE_SIZE, false)?[0].id, text);
        history.reorder(image, text, false, false)?;
        assert_eq!(history.list("", crate::PAGE_SIZE, false)?[0].id, image);
        drop(history);
        assert_eq!(
            History::open(path)?.list("", crate::PAGE_SIZE, false)?[0].id,
            image
        );
        Ok(())
    }

    #[test]
    fn text_eviction_removes_the_oldest_managed_image() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        let image = history.capture_image(PNG, 3, 2, &images)?;
        let path = PathBuf::from(history.item(image)?.image_path.unwrap());
        let connection = db.write_connection();
        connection.lock().execute_batch(
            "WITH RECURSIVE nums(n) AS (
               SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 9999
             )
             INSERT INTO clipboard_items
               (content_type, text_content, content_hash, semantic_hash, created_at)
             SELECT 'text', printf('filler %d', n), printf('filler-hash-%d', n),
                    printf('filler-hash-%d', n), '9999-12-31' FROM nums;",
        )?;
        drop(connection);
        history.capture_with_media("new text", &images)?;
        assert!(history.item(image).is_err());
        assert!(!path.exists());
        Ok(())
    }
}
