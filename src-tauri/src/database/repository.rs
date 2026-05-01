use super::{ContentType, Database};
use crate::clipboard::semantic_hash_from_text;
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
    pub semantic_hash: String,
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
    /// 文件是否存在（仅 files 类型，持久化到数据库）
    #[serde(default)]
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
    pub semantic_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
}

impl Default for NewClipboardItem {
    fn default() -> Self {
        Self {
            content_type: ContentType::Text,
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: None,
            content_hash: String::new(),
            semantic_hash: String::new(),
            preview: None,
            byte_size: 0,
            image_width: None,
            image_height: None,
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOptions {
    pub search: Option<String>,
    pub content_type: Option<String>,
    pub pinned_only: bool,
    pub favorite_only: bool,
    pub tag_id: Option<i64>,
    pub exclude_favorited: bool,
    pub exclude_tagged: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub item_count: i64,
}

/// 剪贴板条目仓库（读写分离）
pub struct ClipboardRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Copy)]
enum HashColumn {
    Content,
    Semantic,
}

impl HashColumn {
    fn as_sql(self) -> &'static str {
        match self {
            HashColumn::Content => "content_hash",
            HashColumn::Semantic => "semantic_hash",
        }
    }
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
            "INSERT INTO clipboard_items 
             (content_type, text_content, html_content, rtf_content, image_path, file_paths, 
              content_hash, semantic_hash, preview, byte_size, image_width, image_height, sort_order, 
              char_count, source_app_name, source_app_icon)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                item.content_type.as_str(),
                item.text_content,
                item.html_content,
                item.rtf_content,
                item.image_path,
                file_paths_json,
                item.content_hash,
                item.semantic_hash,
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
        self.exists_by_column(HashColumn::Content, hash)
    }

    pub fn exists_by_semantic_hash(
        &self,
        hash: &str,
    ) -> Result<bool, rusqlite::Error> {
        self.exists_by_column(HashColumn::Semantic, hash)
    }

    fn exists_by_column(
        &self,
        column: HashColumn,
        hash: &str,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let column = column.as_sql();
        let count: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM clipboard_items WHERE {} = ?1",
                column
            ),
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// 更新已有条目的访问时间并置顶
    pub fn touch_by_hash(&self, hash: &str) -> Result<Option<i64>, rusqlite::Error> {
        self.touch_by_column(HashColumn::Content, hash)
    }

    pub fn touch_by_semantic_hash(
        &self,
        hash: &str,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.touch_by_column(HashColumn::Semantic, hash)
    }

    fn touch_by_column(
        &self,
        column: HashColumn,
        hash: &str,
    ) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.write_conn.lock();

        let max_sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let new_sort = max_sort_order + 1;

        let column = column.as_sql();
        let select_sql = format!(
            "SELECT id FROM clipboard_items \
             WHERE {} = ? \
             ORDER BY sort_order DESC, created_at DESC, id DESC \
             LIMIT 1",
            column
        );

        let target_id: Result<i64, _> =
            conn.query_row(&select_sql, params![hash], |row| row.get(0));

        match target_id {
            Ok(id) => {
                conn.execute(
                    "UPDATE clipboard_items \
                     SET access_count = access_count + 1, \
                         last_accessed_at = datetime('now', 'localtime'), \
                         updated_at = datetime('now', 'localtime'), \
                         created_at = datetime('now', 'localtime'), \
                         sort_order = ?1 \
                     WHERE id = ?2",
                    params![new_sort, id],
                )?;
                Ok(Some(id))
            }
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
             LIMIT 1 OFFSET ?",
            params![index as i64],
            Self::row_to_item,
        );

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 按收藏列表位置获取完整条目，供收藏快速粘贴使用。
    pub fn get_favorite_by_position(&self, index: usize) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let result = conn.query_row(
            "SELECT * FROM clipboard_items \
             WHERE is_favorite = 1 \
             ORDER BY is_pinned DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?",
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
         image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, files_valid";

    /// 搜索查询列（含 text_content 用于关键词上下文预览）
    const SEARCH_COLUMNS: &'static str =
        "id, content_type, text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, files_valid";

    /// 构建通用的 WHERE 条件（content_type / pinned_only / favorite_only / search / tag_id）
    fn build_filter_conditions(
        options: &QueryOptions,
    ) -> (Vec<String>, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // LIKE 搜索（支持中文，匹配全文任意位置）
        if let Some(ref search) = options.search
            && !search.is_empty() {
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

        // 多类型筛选（逗号分隔）
        Self::append_content_type_condition(
            options.content_type.as_deref(),
            &mut conditions,
            &mut params_vec,
        );

        if options.pinned_only {
            conditions.push("is_pinned = 1".to_string());
        }

        if options.favorite_only {
            conditions.push("is_favorite = 1".to_string());
        }

        // 标签过滤：通过 item_tags 关联表筛选
        if let Some(tag_id) = options.tag_id {
            conditions.push(
                "id IN (SELECT item_id FROM item_tags WHERE tag_id = ?)".to_string(),
            );
            params_vec.push(Box::new(tag_id));
        }

        // 排除已收藏条目（主页隐藏已收藏）
        if options.exclude_favorited {
            conditions.push("is_favorite = 0".to_string());
        }

        // 排除有标签的条目（主页隐藏已标记）
        if options.exclude_tagged {
            conditions.push("id NOT IN (SELECT item_id FROM item_tags)".to_string());
        }

        (conditions, params_vec)
    }

    /// 将 content_type（支持逗号分隔）转换为 SQL 条件并追加参数。
    fn append_content_type_condition(
        content_type: Option<&str>,
        conditions: &mut Vec<String>,
        params_vec: &mut Vec<Box<dyn rusqlite::ToSql>>,
    ) {
        let Some(raw) = content_type else {
            return;
        };
        let types: Vec<&str> = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if types.is_empty() {
            return;
        }
        if types.len() == 1 {
            conditions.push("content_type = ?".to_string());
            params_vec.push(Box::new(types[0].to_string()));
        } else {
            let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
            conditions.push(format!("content_type IN ({})", placeholders.join(",")));
            for t in &types {
                params_vec.push(Box::new((*t).to_string()));
            }
        }
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

        // 当按标签过滤时，使用 JOIN 以便按 item_tags.sort_order 排序
        let tag_id_value = options.tag_id;
        let mut opts_for_filter = options.clone();
        if tag_id_value.is_some() {
            opts_for_filter.tag_id = None; // 避免 build_filter_conditions 产生子查询
        }

        // 当按标签过滤时，需要给列名加表前缀以避免歧义
        let prefixed_columns;
        let select_columns = if tag_id_value.is_some() {
            prefixed_columns = columns
                .split(", ")
                .map(|col| {
                    if col.starts_with("NULL ") || col.contains(" AS ") {
                        col.to_string()
                    } else {
                        format!("clipboard_items.{}", col)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            prefixed_columns.as_str()
        } else {
            columns
        };

        let mut sql = format!("SELECT {} FROM clipboard_items", select_columns);
        if tag_id_value.is_some() {
            sql.push_str(
                " INNER JOIN item_tags ON item_tags.item_id = clipboard_items.id AND item_tags.tag_id = ?"
            );
        }

        let (conditions, mut params_vec) = Self::build_filter_conditions(&opts_for_filter);

        // JOIN 参数需要插在 WHERE 参数之前
        if let Some(tid) = tag_id_value {
            params_vec.insert(0, Box::new(tid));
        }

        Self::append_where(&mut sql, &conditions);

        if tag_id_value.is_some() {
            // 标签视图：按 item_tags.sort_order 升序，再按时间降序
            sql.push_str(" ORDER BY item_tags.sort_order ASC, clipboard_items.created_at DESC");
        } else {
            // 排序：置顶优先 → sort_order 降序 → 时间降序
            sql.push_str(" ORDER BY is_pinned DESC, sort_order DESC, created_at DESC");
        }

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

        let mut sql = "SELECT COUNT(*) FROM clipboard_items".to_string();
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

    /// 统计引用同一 image_path 的其他条目数量（排除指定 id）
    pub fn count_image_path_refs(&self, image_path: &str, exclude_id: i64) -> Result<i64, rusqlite::Error> {
        let conn = self.read_conn.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE image_path = ?1 AND id != ?2",
            params![image_path, exclude_id],
            |row| row.get(0),
        )
    }

    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        debug!("Deleted clipboard item with id: {}", id);
        Ok(())
    }

    /// 批量删除指定 ID 的条目，返回被删除条目的图片路径（用于文件清理）
    pub fn batch_delete(&self, ids: &[i64]) -> Result<(i64, Vec<String>), rusqlite::Error> {
        if ids.is_empty() {
            return Ok((0, vec![]));
        }
        let conn = self.write_conn.lock();
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let in_clause = placeholders.join(",");

        let sql = format!(
            "SELECT image_path FROM clipboard_items WHERE id IN ({}) AND image_path IS NOT NULL",
            in_clause
        );
        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let paths: Vec<String> = stmt
            .query_map(params_ref.as_slice(), |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let del_sql = format!("DELETE FROM clipboard_items WHERE id IN ({})", in_clause);
        let params_ref2: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let deleted = conn.execute(&del_sql, params_ref2.as_slice())? as i64;
        debug!("Batch deleted {} clipboard items", deleted);
        Ok((deleted, paths))
    }

    /// 获取可清除条目的图片路径（按类型过滤）
    pub fn get_clearable_image_paths(
        &self,
        content_type: Option<&str>,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut conditions: Vec<String> = vec![
            "is_pinned = 0".to_string(),
            "is_favorite = 0".to_string(),
            "image_path IS NOT NULL".to_string(),
            "id NOT IN (SELECT item_id FROM item_tags)".to_string(),
        ];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        Self::append_content_type_condition(content_type, &mut conditions, &mut params_vec);
        let mut sql = "SELECT image_path FROM clipboard_items".to_string();
        Self::append_where(&mut sql, &conditions);
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let paths: Vec<String> = stmt
            .query_map(params_refs.as_slice(), |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// 清空历史（保留置顶、收藏和有标签的条目），按类型过滤
    pub fn clear_history(
        &self,
        content_type: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut conditions: Vec<String> = vec![
            "is_pinned = 0".to_string(),
            "is_favorite = 0".to_string(),
            "id NOT IN (SELECT item_id FROM item_tags)".to_string(),
        ];
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        Self::append_content_type_condition(content_type, &mut conditions, &mut params_vec);
        let mut sql = "DELETE FROM clipboard_items".to_string();
        Self::append_where(&mut sql, &conditions);
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let deleted = conn.execute(&sql, params_refs.as_slice())?;
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

    /// 获取所有条目的图标路径（含置顶和收藏）
    pub fn get_all_icon_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT source_app_icon FROM clipboard_items WHERE source_app_icon IS NOT NULL",
        )?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// 清空所有历史（包括置顶和收藏），同时删除所有标签数据
    pub fn clear_all(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM item_tags", [])?;
        conn.execute("DELETE FROM tags", [])?;
        let deleted = conn.execute("DELETE FROM clipboard_items", [])?;
        Ok(deleted as i64)
    }

    /// 删除 N 天前的非置顶/非收藏条目，返回 (删除数, 关联图片路径)
    pub fn delete_older_than(&self, days: i64) -> Result<(i64, Vec<String>), rusqlite::Error> {
        let conn = self.write_conn.lock();

        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items \
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL \
             AND created_at < datetime('now', 'localtime', '-' || ? || ' days')",
        )?;
        let image_paths: Vec<String> = stmt
            .query_map(params![days], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let deleted = conn.execute(
            "DELETE FROM clipboard_items \
             WHERE is_pinned = 0 AND is_favorite = 0 \
             AND created_at < datetime('now', 'localtime', '-' || ? || ' days')",
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
            "SELECT image_path FROM clipboard_items \
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL \
             ORDER BY created_at ASC LIMIT ?",
        )?;
        let image_paths: Vec<String> = stmt
            .query_map(params![to_delete], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE id IN (\
                SELECT id FROM clipboard_items \
                WHERE is_pinned = 0 AND is_favorite = 0 \
                ORDER BY created_at ASC LIMIT ?\
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
        let semantic_hash = semantic_hash_from_text(new_text).unwrap_or_else(|| content_hash.clone());

        // 降级为 text 类型，清除 html/rtf 内容
        conn.execute(
            "UPDATE clipboard_items SET text_content = ?1, preview = ?2, content_hash = ?3, semantic_hash = ?4, \
             byte_size = ?5, char_count = ?6, content_type = 'text', \
             html_content = NULL, rtf_content = NULL WHERE id = ?7",
            params![new_text, preview, content_hash, semantic_hash, byte_size, char_count, id],
        )?;
        debug!("Updated text content for item {}", id);
        Ok(())
    }

    /// 将条目移到非置顶区最顶部（粘贴后置顶功能）。
    /// 将 sort_order 设为全表最大值 + 1，由于排序规则是
    /// `is_pinned DESC, sort_order DESC`，置顶条目始终在前，
    /// 本条目将出现在所有非置顶条目的最前面。
    /// 已置顶的条目不作处理，避免打乱用户手动排列的置顶顺序。
    pub fn bump_to_top(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let max_sort: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let affected = conn.execute(
            "UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2 AND is_pinned = 0",
            params![max_sort + 1, id],
        )?;
        if affected > 0 {
            debug!("Bumped item {} to top (sort_order: {})", id, max_sort + 1);
        } else {
            debug!("Skipped bump for item {} (pinned or not found)", id);
        }
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

        // 事务保护原子性
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
            semantic_hash: row.get("semantic_hash")?,
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
            files_valid: row.get("files_valid")?
        })
    }

    /// 查询符合同步条件的条目（按类型过滤 + 大小限制）
    pub fn query_items_for_sync(
        &self,
        type_filter_sql: &str,
        max_byte_size: i64,
    ) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let sql = format!(
            "SELECT * FROM clipboard_items WHERE content_type IN ({}) AND byte_size <= ?1 ORDER BY created_at DESC",
            type_filter_sql
        );
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(params![max_byte_size], Self::row_to_item)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(items)
    }

    /// 查询所有 files 类型条目的 (id, file_paths, current_files_valid)
    pub fn get_file_items_for_validity_check(&self) -> Result<Vec<(i64, String, Option<bool>)>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, file_paths, files_valid FROM clipboard_items WHERE content_type IN ('files', 'video') AND file_paths IS NOT NULL"
        )?;
        let items = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<bool>>(2)?))
        })?.filter_map(|r| r.ok()).collect();
        Ok(items)
    }

    /// 批量更新 files_valid，仅更新实际变化的行，返回变化数
    pub fn batch_update_files_valid(&self, updates: &[(i64, bool)]) -> Result<usize, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut stmt = conn.prepare_cached(
            "UPDATE clipboard_items SET files_valid = ?1 WHERE id = ?2 AND (files_valid IS NULL OR files_valid != ?1)"
        )?;
        let mut changed = 0usize;
        for &(id, valid) in updates {
            if stmt.execute(params![valid, id])? > 0 {
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// 获取所有已失效的 files 类型条目的 file_paths（用于同步过滤）
    pub fn get_invalid_file_paths_set(&self) -> std::collections::HashSet<String> {
        let conn = self.read_conn.lock();
        let mut set = std::collections::HashSet::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT file_paths FROM clipboard_items WHERE content_type IN ('files', 'video') AND files_valid = 0 AND file_paths IS NOT NULL"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    let paths: Vec<String> = serde_json::from_str(&row).unwrap_or_default();
                    for p in paths {
                        set.insert(p);
                    }
                }
            }
        }
        set
    }

    /// 查询所有 files 类型条目的 (file_paths, files_valid)，用于数据大小统计（含失效状态）
    pub fn get_files_stats_with_validity(&self) -> Result<Vec<(String, Option<bool>)>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT file_paths, files_valid FROM clipboard_items WHERE content_type IN ('files', 'video') AND file_paths IS NOT NULL"
        )?;
        let items = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<bool>>(1)?))
        })?.filter_map(|r| r.ok()).collect();
        Ok(items)
    }

    /// 查询所有 files/video 类型条目的 (content_type, file_paths, files_valid)，用于导出
    pub fn get_file_items_for_export(&self) -> Result<Vec<(String, String, Option<bool>)>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT content_type, file_paths, files_valid FROM clipboard_items WHERE content_type IN ('files', 'video') AND file_paths IS NOT NULL"
        )?;
        let items = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<bool>>(2)?))
        })?.filter_map(|r| r.ok()).collect();
        Ok(items)
    }

    /// 导入同步条目（基于 content_hash 去重，已存在则跳过）
    pub fn import_sync_items(&self, items: &[ClipboardItem]) -> Result<usize, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut count = 0usize;

        let max_sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total = items.len() as i64;
        for (i, item) in items.iter().enumerate() {
            // 跳过已存在的条目
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM clipboard_items WHERE content_hash = ?1",
                    params![item.content_hash],
                    |row| row.get(0),
                )
                .unwrap_or(true);

            if exists {
                // 远端标记失效时，同步更新本地的 files_valid 状态
                if (item.content_type == "files" || item.content_type == "video") && item.files_valid == Some(false) {
                    let _ = conn.execute(
                        "UPDATE clipboard_items SET files_valid = 0 WHERE content_hash = ?1 AND (files_valid IS NULL OR files_valid != 0)",
                        params![item.content_hash],
                    );
                }
                continue;
            }

            conn.execute(
                "INSERT INTO clipboard_items
                 (content_type, text_content, html_content, rtf_content, image_path, file_paths,
                  content_hash, semantic_hash, preview, byte_size, image_width, image_height,
                  is_pinned, is_favorite, sort_order, created_at, updated_at,
                  access_count, last_accessed_at, char_count, source_app_name, source_app_icon, files_valid)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
                params![
                    item.content_type,
                    item.text_content,
                    item.html_content,
                    item.rtf_content,
                    item.image_path,
                    item.file_paths,
                    item.content_hash,
                    item.semantic_hash,
                    item.preview,
                    item.byte_size,
                    item.image_width,
                    item.image_height,
                    item.is_pinned,
                    item.is_favorite,
                    max_sort_order + total - i as i64,
                    item.created_at,
                    item.updated_at,
                    item.access_count,
                    item.last_accessed_at,
                    item.char_count,
                    item.source_app_name,
                    item.source_app_icon,
                    item.files_valid,
                ],
            )?;
            count += 1;
        }

        Ok(count)
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

    /// 读取设置值，出错或不存在时返回默认值
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key)
            .ok()
            .flatten()
            .unwrap_or_else(|| default.to_string())
    }

    /// 读取布尔设置，出错或不存在时返回默认值
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key)
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(default)
    }

    /// 清空所有设置
    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM settings", [])?;
        Ok(())
    }
}

/// 标签仓库
pub struct TagRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl TagRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// 列出所有标签（含每个标签的条目数）
    pub fn list_with_count(&self) -> Result<Vec<Tag>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.sort_order, t.created_at, \
             COUNT(it.item_id) AS item_count \
             FROM tags t \
             LEFT JOIN item_tags it ON it.tag_id = t.id \
             GROUP BY t.id \
             ORDER BY t.sort_order ASC, t.created_at ASC",
        )?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    created_at: row.get(3)?,
                    item_count: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// 创建新标签，返回完整标签对象
    pub fn create(&self, name: &str) -> Result<Tag, rusqlite::Error> {
        let conn = self.write_conn.lock();
        // Get next sort_order
        let max_order: i64 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), 0) FROM tags",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        let sort_order = max_order + 1;
        conn.execute(
            "INSERT INTO tags (name, sort_order) VALUES (?1, ?2)",
            params![name, sort_order],
        )?;
        let id = conn.last_insert_rowid();
        let tag = conn.query_row(
            "SELECT id, name, sort_order, created_at FROM tags WHERE id = ?1",
            params![id],
            |row| Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: row.get(3)?,
                item_count: 0,
            }),
        )?;
        debug!("Created tag: id={}, name={}", id, name);
        Ok(tag)
    }

    /// 重命名标签
    pub fn rename(&self, id: i64, name: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        debug!("Renamed tag {} to {}", id, name);
        Ok(())
    }

    /// 删除标签（item_tags 中的关联通过 ON DELETE CASCADE 自动删除）
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        debug!("Deleted tag {}", id);
        Ok(())
    }

    /// 为条目添加标签
    pub fn add_tag_to_item(&self, item_id: i64, tag_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![item_id, tag_id],
        )?;
        debug!("Added tag {} to item {}", tag_id, item_id);
        Ok(())
    }

    /// 从条目移除标签
    pub fn remove_tag_from_item(&self, item_id: i64, tag_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
            params![item_id, tag_id],
        )?;
        debug!("Removed tag {} from item {}", tag_id, item_id);
        Ok(())
    }

    /// 获取条目的所有标签
    pub fn get_tags_for_item(&self, item_id: i64) -> Result<Vec<Tag>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.sort_order, t.created_at, 0 AS item_count \
             FROM tags t \
             INNER JOIN item_tags it ON it.tag_id = t.id \
             WHERE it.item_id = ?1 \
             ORDER BY t.sort_order ASC",
        )?;
        let tags = stmt
            .query_map(params![item_id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    created_at: row.get(3)?,
                    item_count: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// 批量更新标签内条目的排序
    pub fn reorder_tag_items(&self, tag_id: i64, item_ids: &[i64]) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut stmt = conn.prepare(
            "UPDATE item_tags SET sort_order = ?1 WHERE tag_id = ?2 AND item_id = ?3",
        )?;
        for (i, item_id) in item_ids.iter().enumerate() {
            stmt.execute(params![i as i64, tag_id, item_id])?;
        }
        debug!("Reordered {} items in tag {}", item_ids.len(), tag_id);
        Ok(())
    }

    /// 批量更新标签排序
    pub fn reorder_tags(&self, tag_ids: &[i64]) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut stmt = conn.prepare("UPDATE tags SET sort_order = ?1 WHERE id = ?2")?;
        for (i, id) in tag_ids.iter().enumerate() {
            stmt.execute(params![i as i64, id])?;
        }
        debug!("Reordered {} tags", tag_ids.len());
        Ok(())
    }

    /// 导出标签及其条目关联（使用 content_hash 而非 item_id，以便跨设备匹配）
    pub fn export_tags_sync_data(&self) -> Result<TagsSyncData, rusqlite::Error> {
        let conn = self.read_conn.lock();

        // 导出所有标签
        let mut stmt = conn.prepare(
            "SELECT name, sort_order, created_at FROM tags ORDER BY sort_order ASC",
        )?;
        let tags: Vec<TagSyncEntry> = stmt
            .query_map([], |row| {
                Ok(TagSyncEntry {
                    name: row.get(0)?,
                    sort_order: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        // 导出条目-标签关联（用 content_hash 标识条目，含 sort_order）
        let mut stmt = conn.prepare(
            "SELECT ci.content_hash, t.name, it.sort_order \
             FROM item_tags it \
             INNER JOIN clipboard_items ci ON ci.id = it.item_id \
             INNER JOIN tags t ON t.id = it.tag_id",
        )?;
        let associations: Vec<TagAssocSyncEntry> = stmt
            .query_map([], |row| {
                Ok(TagAssocSyncEntry {
                    content_hash: row.get(0)?,
                    tag_name: row.get(1)?,
                    sort_order: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(TagsSyncData { tags, associations })
    }

    /// 导入标签及其条目关联
    pub fn import_tags_sync_data(&self, data: &TagsSyncData) -> Result<usize, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut count = 0usize;

        // 导入标签（按 name 去重，已有则更新 sort_order）
        for tag in &data.tags {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM tags WHERE name = ?1",
                    params![tag.name],
                    |row| row.get(0),
                )
                .unwrap_or(true);

            if !exists {
                conn.execute(
                    "INSERT INTO tags (name, sort_order, created_at) VALUES (?1, ?2, ?3)",
                    params![tag.name, tag.sort_order, tag.created_at],
                )?;
                count += 1;
            } else {
                conn.execute(
                    "UPDATE tags SET sort_order = ?1 WHERE name = ?2",
                    params![tag.sort_order, tag.name],
                )?;
            }
        }

        // 导入条目-标签关联
        let mut assoc_count = 0usize;
        for assoc in &data.associations {
            // 查找本地条目 id（通过 content_hash）
            let item_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM clipboard_items WHERE content_hash = ?1 LIMIT 1",
                    params![assoc.content_hash],
                    |row| row.get(0),
                )
                .ok();

            // 查找本地标签 id（通过 name）
            let tag_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM tags WHERE name = ?1",
                    params![assoc.tag_name],
                    |row| row.get(0),
                )
                .ok();

            if let (Some(item_id), Some(tag_id)) = (item_id, tag_id) {
                let inserted = conn.execute(
                    "INSERT OR IGNORE INTO item_tags (item_id, tag_id, sort_order) VALUES (?1, ?2, ?3)",
                    params![item_id, tag_id, assoc.sort_order],
                )?;
                if inserted > 0 {
                    assoc_count += 1;
                } else {
                    // 已存在则更新 sort_order
                    conn.execute(
                        "UPDATE item_tags SET sort_order = ?1 WHERE item_id = ?2 AND tag_id = ?3",
                        params![assoc.sort_order, item_id, tag_id],
                    )?;
                }
            }
        }

        debug!("Imported {} tags, {} associations", count, assoc_count);
        Ok(count)
    }
}

/// 标签同步数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsSyncData {
    pub tags: Vec<TagSyncEntry>,
    pub associations: Vec<TagAssocSyncEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSyncEntry {
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagAssocSyncEntry {
    pub content_hash: String,
    pub tag_name: String,
    #[serde(default)]
    pub sort_order: i64,
}
