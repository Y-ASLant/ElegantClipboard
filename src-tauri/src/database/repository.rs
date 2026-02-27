use super::{ContentType, Database};
use parking_lot::Mutex;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: i64,
    pub content_type: String,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<String>,
    pub content_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub is_pinned: bool,
    pub is_favorite: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: i64,
    pub last_accessed_at: Option<String>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
    /// 文件是否有效（查询时计算，不存储）
    #[serde(default, skip_deserializing)]
    pub files_valid: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub content_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOptions {
    pub search: Option<String>,
    pub content_type: Option<String>,
    pub pinned_only: bool,
    pub favorite_only: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 剪贴板条目仓库（读写分离）
pub struct ClipboardRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl ClipboardRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    pub fn insert(&self, item: NewClipboardItem) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();

        let file_paths_json = item
            .file_paths
            .map(|paths| serde_json::to_string(&paths).unwrap_or_default());

        let max_sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let new_sort_order = max_sort_order + 1;

        conn.execute(
            "INSERT INTO clipboard_items (content_type, text_content, html_content, rtf_content, image_path, file_paths, content_hash, preview, byte_size, image_width, image_height, sort_order, char_count, source_app_name, source_app_icon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                item.content_type.as_str(),
                item.text_content,
                item.html_content,
                item.rtf_content,
                item.image_path,
                file_paths_json,
                item.content_hash,
                item.preview,
                item.byte_size,
                item.image_width,
                item.image_height,
                new_sort_order,
                item.char_count,
                item.source_app_name,
                item.source_app_icon,
            ],
        )?;

        let id = conn.last_insert_rowid();
        debug!(
            "Inserted clipboard item with id: {}, sort_order: {}",
            id, new_sort_order
        );
        Ok(id)
    }

    pub fn exists_by_hash(&self, hash: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// 更新已有条目的访问时间并置顶
    pub fn touch_by_hash(&self, hash: &str) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.write_conn.lock();

        let max_sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "UPDATE clipboard_items 
             SET access_count = access_count + 1, 
                 last_accessed_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime'),
                 created_at = datetime('now', 'localtime'),
                 sort_order = ?2
             WHERE content_hash = ?1",
            params![hash, max_sort_order + 1],
        )?;

        let result: Result<i64, _> = conn.query_row(
            "SELECT id FROM clipboard_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        );

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_by_id(&self, id: i64) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let result = conn.query_row(
            "SELECT * FROM clipboard_items WHERE id = ?1",
            params![id],
            Self::row_to_item,
        );

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 按默认排序位置获取完整条目（含文本内容），供快速粘贴使用。
    pub fn get_by_position(&self, index: usize) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let result = conn.query_row(
            "SELECT * FROM clipboard_items \
             ORDER BY is_pinned DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?1",
            params![index as i64],
            Self::row_to_item,
        );

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 列表查询列（排除大文本字段以减少 IPC 传输）
    const LIST_COLUMNS: &'static str =
        "id, content_type, NULL AS text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, content_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon";

    /// 搜索查询列（含 text_content 用于关键词上下文预览）
    const SEARCH_COLUMNS: &'static str =
        "id, content_type, text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, content_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon";

    /// 构建通用的 WHERE 条件（content_type / pinned_only / favorite_only / search）
    fn build_filter_conditions(
        options: &QueryOptions,
    ) -> (Vec<String>, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // LIKE 搜索（支持中文，匹配全文任意位置）
        if let Some(ref search) = options.search {
            if !search.is_empty() {
                conditions.push(
                    "(text_content LIKE ? ESCAPE '\\' OR file_paths LIKE ? ESCAPE '\\')"
                        .to_string(),
                );
                let pattern = format!(
                    "%{}%",
                    search
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                params_vec.push(Box::new(pattern.clone()));
                params_vec.push(Box::new(pattern));
            }
        }

        // 支持逗号分隔的多类型筛选（如 "text,html,rtf"）
        if let Some(ref content_type) = options.content_type {
            let types: Vec<&str> = content_type.split(',').map(|s| s.trim()).collect();
            if types.len() == 1 {
                conditions.push("content_type = ?".to_string());
                params_vec.push(Box::new(content_type.clone()));
            } else {
                let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
                conditions.push(format!("content_type IN ({})", placeholders.join(",")));
                for t in &types {
                    params_vec.push(Box::new(t.to_string()));
                }
            }
        }

        if options.pinned_only {
            conditions.push("is_pinned = 1".to_string());
        }

        if options.favorite_only {
            conditions.push("is_favorite = 1".to_string());
        }

        (conditions, params_vec)
    }

    /// 将条件拼接到 SQL 语句
    fn append_where(sql: &mut String, conditions: &[String]) {
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
    }

    pub fn list(&self, options: QueryOptions) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();

        let is_searching = options
            .search
            .as_ref()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let columns = if is_searching {
            Self::SEARCH_COLUMNS
        } else {
            Self::LIST_COLUMNS
        };

        let mut sql = format!("SELECT {} FROM clipboard_items", columns);
        let (conditions, mut params_vec) = Self::build_filter_conditions(&options);
        Self::append_where(&mut sql, &conditions);

        // 排序: 置顶优先 → sort_order 降序 → 时间降序
        sql.push_str(" ORDER BY is_pinned DESC, sort_order DESC, created_at DESC");

        if let Some(limit) = options.limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            params_vec.push(Box::new(limit));
            params_vec.push(Box::new(options.offset.unwrap_or(0)));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(params_refs.as_slice(), Self::row_to_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    pub fn count(&self, options: QueryOptions) -> Result<i64, rusqlite::Error> {
        let conn = self.read_conn.lock();

        let mut sql = String::from("SELECT COUNT(*) FROM clipboard_items");
        let (conditions, params_vec) = Self::build_filter_conditions(&options);
        Self::append_where(&mut sql, &conditions);

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    pub fn toggle_pin(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET is_pinned = NOT is_pinned WHERE id = ?1",
            params![id],
        )?;

        let pinned: bool = conn.query_row(
            "SELECT is_pinned FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok(pinned)
    }

    pub fn toggle_favorite(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET is_favorite = NOT is_favorite WHERE id = ?1",
            params![id],
        )?;

        let favorite: bool = conn.query_row(
            "SELECT is_favorite FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok(favorite)
    }

    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        debug!("Deleted clipboard item with id: {}", id);
        Ok(())
    }

    /// 获取可清除条目的图片路径
    pub fn get_clearable_image_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL",
        )?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// 清空历史（保留置顶和收藏）
    pub fn clear_history(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
        )?;
        Ok(deleted as i64)
    }

    /// 获取所有条目的图片路径（含置顶和收藏）
    pub fn get_all_image_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items WHERE image_path IS NOT NULL",
        )?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// 清空所有历史（包括置顶和收藏）
    pub fn clear_all(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute("DELETE FROM clipboard_items", [])?;
        Ok(deleted as i64)
    }

    /// 删除 N 天前的非置顶/非收藏条目，返回 (删除数, 关联图片路径)
    pub fn delete_older_than(&self, days: i64) -> Result<(i64, Vec<String>), rusqlite::Error> {
        let conn = self.write_conn.lock();

        // 先收集图片路径再执行删除
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 
             AND image_path IS NOT NULL
             AND created_at < datetime('now', 'localtime', '-' || ?1 || ' days')",
        )?;
        let image_paths: Vec<String> = stmt
            .query_map(params![days], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let deleted = conn.execute(
            "DELETE FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 
             AND created_at < datetime('now', 'localtime', '-' || ?1 || ' days')",
            params![days],
        )?;

        debug!("Auto-cleanup: deleted {} items older than {} days", deleted, days);
        Ok((deleted as i64, image_paths))
    }

    /// 执行最大数量限制，返回 (删除数, 图片路径)
    pub fn enforce_max_count(&self, max_count: i64) -> Result<(i64, Vec<String>), rusqlite::Error> {
        if max_count <= 0 {
            return Ok((0, vec![]));
        }

        let conn = self.write_conn.lock();

        let current_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
            |row| row.get(0),
        )?;

        if current_count <= max_count {
            return Ok((0, vec![]));
        }

        let to_delete = current_count - max_count;

        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL
             ORDER BY created_at ASC 
             LIMIT ?1",
        )?;
        let image_paths: Vec<String> = stmt
            .query_map(params![to_delete], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE id IN (
                SELECT id FROM clipboard_items 
                WHERE is_pinned = 0 AND is_favorite = 0 
                ORDER BY created_at ASC 
                LIMIT ?1
            )",
            params![to_delete],
        )?;

        debug!("Enforced max count: deleted {} oldest items", deleted);
        Ok((deleted as i64, image_paths))
    }

    /// 更新文本内容（编辑功能）
    pub fn update_text_content(&self, id: i64, new_text: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let preview: String = new_text.chars().take(200).collect();
        let byte_size = new_text.len() as i64;
        let char_count = new_text.chars().count() as i64;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"text:");
        hasher.update(new_text.as_bytes());
        let content_hash = hasher.finalize().to_hex().to_string();

        // 清除 html/rtf 内容并降级为 text 类型（纯文本编辑后格式内容失效）
        conn.execute(
            "UPDATE clipboard_items SET text_content = ?1, preview = ?2, content_hash = ?3, \
             byte_size = ?4, char_count = ?5, content_type = 'text', \
             html_content = NULL, rtf_content = NULL WHERE id = ?6",
            params![new_text, preview, content_hash, byte_size, char_count, id],
        )?;
        debug!("Updated text content for item {}", id);
        Ok(())
    }

    /// 交换两个条目的排序位置
    pub fn move_item_by_id(&self, from_id: i64, to_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();

        let from_sort_order: i64 = conn.query_row(
            "SELECT sort_order FROM clipboard_items WHERE id = ?1",
            params![from_id],
            |row| row.get(0),
        )?;

        let to_sort_order: i64 = conn.query_row(
            "SELECT sort_order FROM clipboard_items WHERE id = ?1",
            params![to_id],
            |row| row.get(0),
        )?;

        // 使用事务保护两条 UPDATE 的原子性，防止中途失败导致 sort_order 数据损坏
        let tx = conn.unchecked_transaction()?;

        tx.execute(
            "UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2",
            params![to_sort_order, from_id],
        )?;

        tx.execute(
            "UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2",
            params![from_sort_order, to_id],
        )?;

        tx.commit()?;

        debug!(
            "Moved item {} (sort_order: {} -> {}) with item {} (sort_order: {} -> {})",
            from_id, from_sort_order, to_sort_order, to_id, to_sort_order, from_sort_order
        );

        Ok(())
    }

    fn row_to_item(row: &Row) -> Result<ClipboardItem, rusqlite::Error> {
        Ok(ClipboardItem {
            id: row.get("id")?,
            content_type: row.get("content_type")?,
            text_content: row.get("text_content")?,
            html_content: row.get("html_content")?,
            rtf_content: row.get("rtf_content")?,
            image_path: row.get("image_path")?,
            file_paths: row.get("file_paths")?,
            content_hash: row.get("content_hash")?,
            preview: row.get("preview")?,
            byte_size: row.get("byte_size")?,
            image_width: row.get("image_width")?,
            image_height: row.get("image_height")?,
            is_pinned: row.get("is_pinned")?,
            is_favorite: row.get("is_favorite")?,
            sort_order: row.get("sort_order")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            access_count: row.get("access_count")?,
            last_accessed_at: row.get("last_accessed_at")?,
            char_count: row.get("char_count")?,
            source_app_name: row.get("source_app_name")?,
            source_app_icon: row.get("source_app_icon")?,
            files_valid: None, // 查询时计算
        })
    }
}

/// 设置仓库
pub struct SettingsRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl SettingsRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_all(&self) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let settings = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(settings)
    }

    /// 清空所有设置
    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM settings", [])?;
        Ok(())
    }
}
