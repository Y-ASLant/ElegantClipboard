//! Shared clipboard-history use cases. No dependency on a desktop UI or platform API.
pub mod backup;
pub(crate) mod backup_common;
mod blob_store;
pub(crate) mod clipboard;
pub mod database;
mod editing;
mod file_staging;
mod files;
mod groups;
pub mod html_text;
mod image;
pub mod import;
pub mod legacy_backup;
mod merge;
pub(crate) mod native_format;
pub mod preferences;
pub(crate) mod preview;
mod reorder;
pub mod retention;
pub mod rich;
pub(crate) mod same_text;
pub mod search;
pub use file_staging::{FileCaptureOutcome, MAX_STAGED_DIRECTORY_BYTES, MAX_STAGED_FILE_BYTES};
pub use files::{MAX_FILE_PATHS, MAX_PATH_LIST_BYTES};
pub use image::{DeletionOutcome, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
pub use merge::MergedContent;
pub use native_format::NativeFormatCapture;
pub use preview::{FilePreviewEntry, PreviewContent};

/// 平台层采集与写回使用的原生格式限额。
pub mod native_format_public {
    pub use crate::native_format::{
        MAX_NATIVE_FORMAT_BYTES, MAX_NATIVE_FORMAT_TOTAL_BYTES, NATIVE_INLINE_MAX_BYTES,
        valid_format_name,
    };
}

/// 平台层启动清扫使用的 blob 存储入口。
pub mod blob_store_public {
    pub use crate::blob_store::{BlobStore, sweep_pending_at};
}

/// 计算数据目录对应的 blob 存储根目录。
pub fn blob_root_for(db_path: &std::path::Path) -> Option<std::path::PathBuf> {
    db_path.parent().map(|parent| parent.join("blobs"))
}

use anyhow::{Context, Result, bail};
use database::{
    ClipboardItem, ClipboardRepository, ContentType, Database, Group, GroupRepository,
    NewClipboardItem, QueryOptions,
};
use std::path::{Path, PathBuf};

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const HISTORY_LIMIT: i64 = 10_000;
pub const PAGE_SIZE: i64 = 100;

pub fn is_url_text(text: &str) -> bool {
    clipboard::is_url(text)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContentCategory {
    #[default]
    All,
    Text,
    Other,
}

impl ContentCategory {
    fn content_types(self) -> &'static str {
        match self {
            Self::All => "text,url,html,rtf,image,files",
            Self::Text => "text,html,rtf",
            Self::Other => "image,files,url",
        }
    }
}

pub struct History {
    repo: ClipboardRepository,
    db: Database,
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
        self.reorder_in_group(from, to, after, favorite_only, None)
    }

    pub fn reorder_in_group(
        &self,
        from: i64,
        to: i64,
        after: bool,
        favorite_only: bool,
        group_id: Option<i64>,
    ) -> Result<()> {
        reorder::move_item(&self.db, from, to, after, favorite_only, group_id)
    }

    pub fn groups(&self) -> Result<Vec<Group>> {
        Ok(GroupRepository::new(&self.db).list_with_count()?)
    }

    pub fn reorder_group(&self, from: i64, to: i64, after: bool) -> Result<()> {
        groups::reorder(&self.db, from, to, after)
    }

    pub fn create_group(&self, name: &str) -> Result<Group> {
        let name = Self::checked_group_name(name)?;
        if self.groups()?.iter().any(|group| group.name == name) {
            bail!("分组名称已存在");
        }
        Ok(GroupRepository::new(&self.db).create(name, None)?)
    }

    pub fn rename_group(&self, id: i64, name: &str) -> Result<Group> {
        let name = Self::checked_group_name(name)?;
        let groups = self.groups()?;
        let current = groups
            .iter()
            .find(|group| group.id == id)
            .ok_or_else(|| anyhow::anyhow!("分组已不存在"))?;
        if groups
            .iter()
            .any(|group| group.id != id && group.name == name)
        {
            bail!("分组名称已存在");
        }
        if current.name == name {
            return Ok(current.clone());
        }
        GroupRepository::new(&self.db).rename(id, name)?;
        Ok(Group {
            name: name.to_owned(),
            ..current.clone()
        })
    }

    pub fn delete_group_preserving_items(&self, id: i64) -> Result<usize> {
        groups::delete_preserving_items(&self.db, id)
    }

    fn checked_group_name(name: &str) -> Result<&str> {
        let name = name.trim();
        if name.is_empty() {
            bail!("请输入分组名称");
        }
        if name.chars().count() > 40 || name.len() > 160 {
            bail!("分组名称不能超过 40 个字符或 160 字节");
        }
        Ok(name)
    }

    pub fn move_to_group(
        &self,
        id: i64,
        source_group_id: Option<i64>,
        target_group_id: Option<i64>,
    ) -> Result<()> {
        let item = self.item(id)?;
        if item.group_id != source_group_id {
            bail!("记录已经移动，请刷新列表后重试");
        }
        if source_group_id == target_group_id {
            return Ok(());
        }
        if let Some(target) = target_group_id
            && !self.groups()?.iter().any(|group| group.id == target)
        {
            bail!("目标分组已不存在");
        }
        GroupRepository::new(&self.db).move_item_to_group(id, target_group_id)?;
        Ok(())
    }

    pub fn capture(&self, text: &str) -> Result<Option<i64>> {
        self.capture_inner(text, None)
    }

    pub fn capture_with_media(&self, text: &str, images_dir: &Path) -> Result<Option<i64>> {
        self.capture_inner(text, Some(images_dir))
    }

    pub fn set_source_app(&self, id: i64, name: &str, icon: Option<&str>) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            bail!("来源应用名称无效");
        }
        if !self.repo.set_source_app(id, name, icon)? {
            bail!("历史记录已不存在");
        }
        Ok(())
    }

    pub fn bump_to_top(&self, id: i64) -> Result<()> {
        self.repo.bump_to_top(id)?;
        Ok(())
    }

    pub fn quick_paste_item_id(
        &self,
        slot: u8,
        favorite: bool,
        group_id: Option<i64>,
    ) -> Result<Option<i64>> {
        if !(1..=10).contains(&slot) {
            bail!("快速粘贴槽位必须在 1 到 10 之间");
        }
        let index = usize::from(slot - 1);
        Ok(if favorite {
            self.repo.get_favorite_id_by_position(index, group_id)?
        } else {
            self.repo.get_id_by_position(index, group_id)?
        })
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
                (ContentType::Url, url, b"url:".as_slice())
            } else {
                (ContentType::Text, text, b"text:".as_slice())
            };
        let hash = clipboard::hash_with_prefix(hash_prefix, text.as_bytes());
        // URL trimming matches the legacy database; other text keeps exact bytes.
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(Some(id));
        }
        // 同文合并：默认分组内已有相同纯文本的条目时并入而不新建。
        if let Some(target) = self.find_same_text_item(text)? {
            let input = same_text::MergeInput {
                content_type,
                html: None,
                rtf: None,
                text: Some(text),
                hash,
            };
            return Ok(Some(self.merge_same_text(&target, &input, images_dir)?));
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
        self.enforce_history_limit(images_dir)?;
        Ok(Some(id))
    }

    pub fn list(
        &self,
        search: &str,
        limit: i64,
        favorite_only: bool,
    ) -> Result<Vec<ClipboardItem>> {
        self.list_in_group(search, limit, favorite_only, None)
    }

    pub fn list_in_group(
        &self,
        search: &str,
        limit: i64,
        favorite_only: bool,
        group_id: Option<i64>,
    ) -> Result<Vec<ClipboardItem>> {
        self.list_filtered_in_group(search, limit, favorite_only, group_id, ContentCategory::All)
    }

    pub fn list_filtered_in_group(
        &self,
        search: &str,
        limit: i64,
        favorite_only: bool,
        group_id: Option<i64>,
        category: ContentCategory,
    ) -> Result<Vec<ClipboardItem>> {
        Ok(self.repo.list(QueryOptions {
            search: (!search.is_empty()).then(|| search.to_owned()),
            content_type: Some(category.content_types().into()),
            favorite_only,
            group_id,
            limit: Some(limit.clamp(PAGE_SIZE, HISTORY_LIMIT)),
            ..Default::default()
        })?)
    }

    pub fn count(&self, search: &str, favorite_only: bool) -> Result<i64> {
        self.count_in_group(search, favorite_only, None)
    }

    pub fn count_in_group(
        &self,
        search: &str,
        favorite_only: bool,
        group_id: Option<i64>,
    ) -> Result<i64> {
        self.count_filtered_in_group(search, favorite_only, group_id, ContentCategory::All)
    }

    pub fn count_filtered_in_group(
        &self,
        search: &str,
        favorite_only: bool,
        group_id: Option<i64>,
        category: ContentCategory,
    ) -> Result<i64> {
        Ok(self.repo.count(QueryOptions {
            search: (!search.is_empty()).then(|| search.to_owned()),
            content_type: Some(category.content_types().into()),
            favorite_only,
            group_id,
            ..Default::default()
        })?)
    }
    /// Daily counts of retained history items across all groups from a local date onward.
    pub fn daily_counts(&self, start_date: &str) -> Result<Vec<(String, i64)>> {
        Ok(self.repo.daily_counts(start_date)?)
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

    pub fn optimize_storage(&self) -> Result<()> {
        self.db.optimize().context("更新数据库统计失败")?;
        self.db.vacuum().context("回收数据库空闲空间失败")?;
        let busy: i64 = self
            .db
            .write_connection()
            .lock()
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .context("合并数据库日志失败")?;
        if busy != 0 {
            bail!("数据库日志正在使用，无法完成空间回收");
        }
        Ok(())
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
mod tests;
