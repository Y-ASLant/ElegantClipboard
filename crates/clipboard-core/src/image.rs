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
        let hash = crate::clipboard::hash_with_prefix(b"image:", png);
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
        self.enforce_history_limit(Some(images_dir))?;
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
                .as_deref()
                == Some(root.as_path())
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
mod tests;
