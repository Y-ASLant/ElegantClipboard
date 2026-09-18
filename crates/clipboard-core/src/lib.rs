//! Shared clipboard-history use cases. No dependency on a desktop UI or platform API.
//!
//! Transitional source bridge: compile the existing schema/repository and dedup
//! algorithms directly so the two entry points cannot acquire divergent copies.
//! Their physical move is deferred until the old shell adopts this crate.
#[allow(dead_code)] // Rich-format helpers are shared with the legacy application.
#[path = "../../../src-tauri/src/clipboard/dedup.rs"]
pub(crate) mod clipboard;
#[path = "../../../src-tauri/src/database/mod.rs"]
pub mod database;
mod image;
pub mod import;
pub mod preferences;
mod reorder;
pub use image::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};

use anyhow::{Result, bail};
use database::{
    ClipboardItem, ClipboardRepository, ContentType, Database, NewClipboardItem, QueryOptions,
};
use std::path::{Path, PathBuf};

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const HISTORY_LIMIT: i64 = 10_000;
pub const PAGE_SIZE: i64 = 100;

pub struct History {
    repo: ClipboardRepository,
    db: Database,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewContent {
    Text(String),
    Image(PathBuf),
}

impl History {
    pub fn open(path: PathBuf) -> Result<Self> {
        let db = Database::new(path)?;
        Ok(Self::new(&db))
    }

    pub fn new(db: &Database) -> Self {
        Self {
            repo: ClipboardRepository::new(db),
            db: db.clone(),
        }
    }

    pub fn reorder(&self, from: i64, to: i64, after: bool, favorite_only: bool) -> Result<()> {
        reorder::move_item(&self.db, from, to, after, favorite_only)
    }

    pub fn capture(&self, text: &str) -> Result<Option<i64>> {
        self.capture_inner(text, None)
    }

    pub fn capture_with_media(&self, text: &str, images_dir: &Path) -> Result<Option<i64>> {
        self.capture_inner(text, Some(images_dir))
    }

    fn capture_inner(&self, text: &str, images_dir: Option<&Path>) -> Result<Option<i64>> {
        if text.trim().is_empty() {
            return Ok(None);
        }
        if text.len() > MAX_TEXT_BYTES {
            bail!("文本超过 1 MiB，未保存");
        }
        let (content_type, text, hash_prefix) =
            if let Some(url) = clipboard::canonical_url_text(text) {
                (ContentType::Url, url, "url:")
            } else {
                (ContentType::Text, text, "text:")
            };
        let hash = blake3::hash(format!("{hash_prefix}{text}").as_bytes())
            .to_hex()
            .to_string();
        // URL trimming matches the legacy database; other text keeps exact bytes.
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(Some(id));
        }
        let id = self.repo.insert(NewClipboardItem {
            content_type,
            text_content: Some(text.to_owned()),
            content_hash: hash.clone(),
            semantic_hash: clipboard::semantic_hash_from_text(text).unwrap_or(hash),
            preview: Some(text.chars().take(240).collect()),
            byte_size: text.len() as i64,
            char_count: Some(text.chars().count() as i64),
            ..Default::default()
        })?;
        let (_, deleted_images, _) = self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        if let Some(images_dir) = images_dir {
            self.cleanup_images(deleted_images, images_dir);
        }
        Ok(Some(id))
    }

    pub fn list(
        &self,
        search: &str,
        limit: i64,
        favorite_only: bool,
    ) -> Result<Vec<ClipboardItem>> {
        Ok(self.repo.list(QueryOptions {
            search: (!search.is_empty()).then(|| search.to_owned()),
            content_type: Some("text,url,image".into()),
            favorite_only,
            limit: Some(limit.clamp(PAGE_SIZE, HISTORY_LIMIT)),
            ..Default::default()
        })?)
    }

    pub fn count(&self, search: &str, favorite_only: bool) -> Result<i64> {
        Ok(self.repo.count(QueryOptions {
            search: (!search.is_empty()).then(|| search.to_owned()),
            content_type: Some("text,url,image".into()),
            favorite_only,
            ..Default::default()
        })?)
    }

    pub fn text(&self, id: i64) -> Result<String> {
        self.repo
            .get_by_id(id)?
            .and_then(|item| item.text_content)
            .ok_or_else(|| anyhow::anyhow!("记录已不存在"))
    }

    pub fn item(&self, id: i64) -> Result<ClipboardItem> {
        self.repo
            .get_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("记录已不存在"))
    }

    pub fn preview_content(&self, id: i64) -> Result<PreviewContent> {
        let item = self.item(id)?;
        if item.content_type == "image" {
            let path = PathBuf::from(
                item.image_path
                    .ok_or_else(|| anyhow::anyhow!("图片路径缺失"))?,
            );
            if !path.is_file() {
                bail!("图片文件已丢失，无法预览");
            }
            Ok(PreviewContent::Image(path))
        } else {
            Ok(PreviewContent::Text(
                item.text_content
                    .ok_or_else(|| anyhow::anyhow!("记录没有可预览的文本"))?,
            ))
        }
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        Ok(self.repo.delete(id)?)
    }

    pub fn toggle_favorite(&self, id: i64) -> Result<bool> {
        Ok(self.repo.toggle_favorite(id)?)
    }

    pub fn toggle_pin(&self, id: i64) -> Result<bool> {
        Ok(self.repo.toggle_pin(id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_persist_filter_search_and_keep_full_history() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let id = history.capture("收藏中文%_😀")?.unwrap();
        history.capture("普通中文%_😀")?;
        assert!(history.toggle_favorite(id)?);
        assert_eq!(history.count("%_", true)?, 1);
        assert_eq!(history.list("%_", PAGE_SIZE, true)?[0].id, id);
        assert_eq!(history.count("%_", false)?, 2);
        assert!(history.list("不存在", PAGE_SIZE, true)?.is_empty());
        drop(history);
        let history = History::open(path)?;
        assert_eq!(history.count("", true)?, 1);
        assert_eq!(history.capture("收藏中文%_😀")?, Some(id));
        assert_eq!(history.count("", true)?, 1);
        assert!(!history.toggle_favorite(id)?);
        assert!(history.list("", PAGE_SIZE, true)?.is_empty());
        assert_eq!(history.count("", false)?, 2);
        assert!(history.toggle_favorite(-1).is_err());
        Ok(())
    }

    #[test]
    fn history_persists_deduplicates_and_preserves_full_text() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let text = format!("中文😀\r\n{}", "a".repeat(600));
        let id;
        {
            let history = History::open(path.clone())?;
            id = history.capture(&text)?.unwrap();
            history.capture("second")?;
            assert_eq!(history.capture(&text)?, Some(id));
            assert_eq!(history.count("", false)?, 2);
            assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
        }
        let history = History::open(path)?;
        assert_eq!(history.text(id)?, text);
        assert_eq!(history.count("中文😀", false)?, 1);
        Ok(())
    }

    #[test]
    fn imported_urls_are_visible_and_keep_legacy_dedup_identity() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let url = "https://example.com/旧记录";
        let hash = blake3::hash(format!("url:{url}").as_bytes())
            .to_hex()
            .to_string();
        let id = ClipboardRepository::new(&db).insert(NewClipboardItem {
            content_type: ContentType::Url,
            text_content: Some(url.into()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            preview: Some(url.into()),
            ..Default::default()
        })?;
        let history = History::new(&db);
        assert_eq!(history.count("example.com", false)?, 1);
        assert_eq!(history.list("旧记录", PAGE_SIZE, false)?[0].id, id);
        assert_eq!(history.capture(&format!("  {url}  "))?, Some(id));
        assert_eq!(history.count("", false)?, 1);
        assert_eq!(history.text(id)?, url);
        Ok(())
    }

    #[test]
    fn literal_search_pin_delete_and_limits() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let id = history.capture("100%_done")?.unwrap();
        history.capture("plain text")?;
        assert_eq!(history.count("%_", false)?, 1);
        history.toggle_pin(id)?;
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
        assert_eq!(history.capture(" \r\n")?, None);
        assert!(history.capture(&"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
        history.delete(id)?;
        assert!(history.text(id).is_err());
        assert_eq!(history.count("", false)?, 1);
        Ok(())
    }
}
