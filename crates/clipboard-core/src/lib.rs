//! Shared clipboard-history use cases. No dependency on a desktop UI or platform API.
pub mod backup;
#[allow(dead_code)]
pub(crate) mod clipboard;
pub mod database;
mod editing;
mod files;
mod groups;
mod image;
pub mod import;
pub mod legacy_backup;
mod merge;
pub mod preferences;
mod reorder;
pub mod rich;
pub use files::{MAX_FILE_PATHS, MAX_PATH_LIST_BYTES};
pub use image::{MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS};
pub use merge::MergedContent;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreviewEntry {
    pub original_path: String,
    pub resolved_path: String,
    pub exists: bool,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub recovered: bool,
    pub metadata_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewContent {
    Text(String),
    Image(PathBuf),
    Files(Vec<FilePreviewEntry>),
    RichText(String),
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
        let item = if favorite {
            self.repo.get_favorite_by_position(index, group_id)?
        } else {
            self.repo.get_by_position(index, group_id)?
        };
        Ok(item.map(|item| item.id))
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
        let (_, deleted_images, deleted_payloads) =
            self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        if let Some(images_dir) = images_dir {
            self.cleanup_images(deleted_images, images_dir);
            self.cleanup_staged(deleted_payloads, images_dir);
        }
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

    pub fn preview_content(&self, id: i64) -> Result<PreviewContent> {
        self.preview_content_inner(id, None)
    }

    pub fn preview_content_with_staged(
        &self,
        id: i64,
        staged_dir: &Path,
    ) -> Result<PreviewContent> {
        self.preview_content_inner(id, Some(staged_dir))
    }

    fn preview_content_inner(&self, id: i64, staged_dir: Option<&Path>) -> Result<PreviewContent> {
        let item = self.item(id)?;
        if item.content_type == "files" {
            return Ok(PreviewContent::Files(
                self.file_preview_entries(id, staged_dir)?,
            ));
        }
        if matches!(item.content_type.as_str(), "html" | "rtf") {
            return Ok(PreviewContent::RichText(
                item.text_content
                    .or(item.preview)
                    .unwrap_or_else(|| "[富文本内容，无纯文本预览]".into()),
            ));
        }
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
    fn database_maintenance_reclaims_free_pages_and_preserves_history() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("clipboard.db");
        let history = History::open(database.clone())?;
        let id = history.capture("keep after maintenance")?.unwrap();
        {
            let connection = history.db.write_connection();
            let connection = connection.lock();
            connection.execute_batch(
                "CREATE TABLE maintenance_fixture (payload BLOB NOT NULL);
                 INSERT INTO maintenance_fixture(payload) VALUES (zeroblob(4194304));
                 DELETE FROM maintenance_fixture;
                 PRAGMA wal_checkpoint(TRUNCATE);",
            )?;
        }
        let before = std::fs::metadata(&database)?.len();

        history.optimize_storage()?;

        let after = std::fs::metadata(&database)?.len();
        assert!(after < before, "before={before}, after={after}");
        assert_eq!(history.text(id)?, "keep after maintenance");
        Ok(())
    }

    #[test]
    fn group_creation_validates_names_and_persists() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        assert!(history.create_group(" \n ").is_err());
        assert!(history.create_group(&"名".repeat(41)).is_err());
        let group = history.create_group("  工作  ")?;
        assert_eq!(group.name, "工作");
        assert!(history.create_group("工作").is_err());
        drop(history);
        assert_eq!(History::open(path)?.groups()?[0].id, group.id);
        Ok(())
    }

    #[test]
    fn group_rename_preserves_membership_and_rejects_duplicates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let work = history.create_group("工作")?;
        history.create_group("归档")?;
        let id = history.capture("work item")?.unwrap();
        history.move_to_group(id, None, Some(work.id))?;
        assert!(history.rename_group(work.id, "归档").is_err());
        assert!(history.rename_group(-1, "其他").is_err());
        assert!(history.rename_group(work.id, " ").is_err());
        let renamed = history.rename_group(work.id, "  常用  ")?;
        assert_eq!(renamed.name, "常用");
        assert_eq!(history.item(id)?.group_id, Some(work.id));
        drop(history);
        assert_eq!(History::open(path)?.groups()?[0].name, "常用");
        Ok(())
    }

    #[test]
    fn moving_items_between_groups_preserves_history_and_rejects_stale_source() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let id = history.capture("important text")?.unwrap();
        history.toggle_favorite(id)?;
        history.toggle_pin(id)?;
        let work = history.create_group("工作")?;
        let archive = history.create_group("归档")?;
        assert!(history.move_to_group(id, None, Some(-1)).is_err());
        history.move_to_group(id, None, Some(work.id))?;
        assert!(history.list("", PAGE_SIZE, false)?.is_empty());
        assert_eq!(
            history.list_in_group("", PAGE_SIZE, false, Some(work.id))?[0].id,
            id
        );
        assert!(history.move_to_group(id, None, Some(archive.id)).is_err());
        history.move_to_group(id, Some(work.id), Some(archive.id))?;
        assert_eq!(history.count_in_group("", false, Some(work.id))?, 0);
        let item = history.item(id)?;
        assert_eq!(item.group_id, Some(archive.id));
        assert!(item.is_pinned && item.is_favorite);
        history.move_to_group(id, Some(archive.id), None)?;
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
        Ok(())
    }

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
    fn category_queries_filter_search_count_and_group() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let repo = ClipboardRepository::new(&db);
        let mut ids = Vec::new();
        for (index, kind) in [
            ContentType::Text,
            ContentType::Html,
            ContentType::Rtf,
            ContentType::Image,
            ContentType::Files,
            ContentType::Url,
        ]
        .into_iter()
        .enumerate()
        {
            ids.push(repo.insert(NewClipboardItem {
                content_type: kind,
                text_content: Some(format!("needle-{index}")),
                content_hash: format!("category-{index}"),
                semantic_hash: format!("category-{index}"),
                ..Default::default()
            })?);
        }
        let history = History::new(&db);
        assert_eq!(
            history.count_filtered_in_group("needle", false, None, ContentCategory::Text)?,
            3
        );
        assert_eq!(
            history
                .list_filtered_in_group("", PAGE_SIZE, false, None, ContentCategory::Other)?
                .len(),
            3
        );
        history.toggle_favorite(ids[1])?;
        assert_eq!(
            history.count_filtered_in_group("", true, None, ContentCategory::Text)?,
            1
        );
        let group = history.create_group("archive")?;
        history.move_to_group(ids[5], None, Some(group.id))?;
        assert_eq!(
            history.count_filtered_in_group("", false, None, ContentCategory::Other)?,
            2
        );
        assert_eq!(
            history.list_filtered_in_group(
                "needle",
                PAGE_SIZE,
                false,
                Some(group.id),
                ContentCategory::Other
            )?[0]
                .id,
            ids[5]
        );
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
