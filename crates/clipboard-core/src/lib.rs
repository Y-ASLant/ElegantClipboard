//! Shared text-history use cases. No dependency on a desktop UI or platform API.
//!
//! Transitional source bridge: compile the existing schema/repository and dedup
//! algorithms directly so the two entry points cannot acquire divergent copies.
//! Their physical move is deferred until the old shell adopts this crate.
#[allow(dead_code)] // Rich-format helpers are shared with the legacy application.
#[path = "../../../src-tauri/src/clipboard/dedup.rs"]
pub(crate) mod clipboard;
#[path = "../../../src-tauri/src/database/mod.rs"]
pub mod database;
pub mod preferences;

use anyhow::{Result, bail};
use database::{
    ClipboardItem, ClipboardRepository, ContentType, Database, NewClipboardItem, QueryOptions,
};
use std::path::PathBuf;

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const HISTORY_LIMIT: i64 = 10_000;
pub const PAGE_SIZE: i64 = 100;

pub struct History {
    repo: ClipboardRepository,
}

impl History {
    pub fn open(path: PathBuf) -> Result<Self> {
        let db = Database::new(path)?;
        Ok(Self::new(&db))
    }

    pub fn new(db: &Database) -> Self {
        Self {
            repo: ClipboardRepository::new(db),
        }
    }

    pub fn capture(&self, text: &str) -> Result<Option<i64>> {
        if text.trim().is_empty() {
            return Ok(None);
        }
        if text.len() > MAX_TEXT_BYTES {
            bail!("文本超过 1 MiB，未保存");
        }
        let hash = blake3::hash(format!("text:{text}").as_bytes())
            .to_hex()
            .to_string();
        // Exact dedup preserves different Unicode and whitespace content in this MVP.
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(Some(id));
        }
        let id = self.repo.insert(NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(text.to_owned()),
            content_hash: hash.clone(),
            semantic_hash: clipboard::semantic_hash_from_text(text).unwrap_or(hash),
            preview: Some(text.chars().take(240).collect()),
            byte_size: text.len() as i64,
            char_count: Some(text.chars().count() as i64),
            ..Default::default()
        })?;
        self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
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
            content_type: Some("text".into()),
            favorite_only,
            limit: Some(limit.clamp(PAGE_SIZE, HISTORY_LIMIT)),
            ..Default::default()
        })?)
    }

    pub fn count(&self, search: &str, favorite_only: bool) -> Result<i64> {
        Ok(self.repo.count(QueryOptions {
            search: (!search.is_empty()).then(|| search.to_owned()),
            content_type: Some("text".into()),
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
