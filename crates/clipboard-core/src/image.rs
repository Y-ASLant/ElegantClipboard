use crate::{
    HISTORY_LIMIT, History,
    database::{ContentType, NewClipboardItem},
};
use anyhow::{Context, Result, bail};
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
        let (_, deleted_images, _) = self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        self.cleanup_images(deleted_images, images_dir);
        Ok(id)
    }

    pub fn delete_with_media(&self, id: i64, images_dir: &Path) -> Result<()> {
        let image_path = self.repo.get_by_id(id)?.and_then(|item| item.image_path);
        self.repo.delete(id)?;
        if let Some(image_path) = image_path {
            self.cleanup_images(vec![image_path], images_dir);
        }
        Ok(())
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
