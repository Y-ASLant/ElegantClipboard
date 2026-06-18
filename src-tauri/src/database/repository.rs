use super::{ContentType, Database};
use crate::clipboard::semantic_hash_from_text;
use parking_lot::Mutex;
use rusqlite::{Connection, Row, Transaction, params};
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
    pub favorite_order: i64,
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
    pub semantic_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
    /// None = 默认分组，Some(id) = 自定义分组
    pub group_id: Option<i64>,
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
            group_id: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOptions {
    pub search: Option<String>,
    pub content_type: Option<String>,
    pub pinned_only: bool,
    pub favorite_only: bool,
    pub group_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
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

/// Dynamic SQL condition builder to reduce boilerplate in repository query methods.
///
/// Replaces repetitive patterns of condition assembly, parameter boxing, and
/// reference conversion with a fluent builder API.
struct ConditionBuilder {
    conditions: Vec<String>,
    params: Vec<Box<dyn rusqlite::ToSql>>,
}

impl ConditionBuilder {
    fn new() -> Self {
        Self {
            conditions: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Add non-pinned + non-favorite conditions (clearable items).
    fn clearable(mut self) -> Self {
        self.conditions.push("is_pinned = 0".to_string());
        self.conditions.push("is_favorite = 0".to_string());
        self
    }

    /// Add group_id filter (NULL = default group, Some(id) = custom group).
    fn group(mut self, group_id: Option<i64>) -> Self {
        let (cond, param) = ClipboardRepository::group_condition(group_id);
        self.conditions.push(cond.to_string());
        if let Some(gid) = param {
            self.params.push(Box::new(gid));
        }
        self
    }

    /// Add content_type filter (supports comma-separated multi-type).
    fn content_type(mut self, content_type: Option<&str>) -> Self {
        ClipboardRepository::append_content_type_condition(
            content_type,
            &mut self.conditions,
            &mut self.params,
        );
        self
    }

    /// Add a condition without an associated parameter.
    fn condition(mut self, cond: &str) -> Self {
        self.conditions.push(cond.to_string());
        self
    }

    /// Add a condition with an associated parameter.
    fn condition_with_param(mut self, cond: &str, value: impl rusqlite::ToSql + 'static) -> Self {
        self.conditions.push(cond.to_string());
        self.params.push(Box::new(value));
        self
    }

    /// Push a trailing parameter (e.g., for LIMIT clauses outside WHERE).
    fn param(mut self, value: impl rusqlite::ToSql + 'static) -> Self {
        self.params.push(Box::new(value));
        self
    }

    /// Build the WHERE clause (includes leading ` WHERE `).
    fn where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// Get parameter references for query execution.
    fn param_refs(&self) -> Vec<&dyn rusqlite::ToSql> {
        self.params.iter().map(|p| p.as_ref()).collect()
    }

    /// SELECT single-column string results with optional trailing SQL.
    fn select_strings(
        &self,
        conn: &Connection,
        prefix: &str,
        suffix: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let sql = if suffix.is_empty() {
            format!("{}{}", prefix, self.where_clause())
        } else {
            format!("{}{} {}", prefix, self.where_clause(), suffix)
        };
        let refs = self.param_refs();
        let mut stmt = conn.prepare(&sql)?;
        let results = stmt
            .query_map(refs.as_slice(), |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    /// DELETE FROM clipboard_items and return affected row count.
    fn delete_items(&self, conn: &Connection) -> Result<i64, rusqlite::Error> {
        let sql = format!("DELETE FROM clipboard_items{}", self.where_clause());
        let refs = self.param_refs();
        Ok(conn.execute(&sql, refs.as_slice())? as i64)
    }

    /// COUNT(*) on clipboard_items.
    fn count_items(&self, conn: &Connection) -> Result<i64, rusqlite::Error> {
        let sql = format!(
            "SELECT COUNT(*) FROM clipboard_items{}",
            self.where_clause()
        );
        let refs = self.param_refs();
        conn.query_row(&sql, refs.as_slice(), |row| row.get(0))
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
              char_count, source_app_name, source_app_icon, group_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
                item.group_id,
            ],
        )?;

        let id = conn.last_insert_rowid();
        debug!(
            "Inserted clipboard item with id: {}, sort_order: {}, group_id: {:?}",
            id, new_sort_order, item.group_id
        );
        Ok(id)
    }

    pub fn exists_by_hash(
        &self,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<bool, rusqlite::Error> {
        self.exists_by_column(HashColumn::Content, hash, group_id)
    }

    pub fn exists_by_semantic_hash(
        &self,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<bool, rusqlite::Error> {
        self.exists_by_column(HashColumn::Semantic, hash, group_id)
    }

    fn exists_by_column(
        &self,
        column: HashColumn,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<bool, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let column = column.as_sql();
        let count: i64 = match group_id {
            Some(gid) => conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM clipboard_items WHERE {} = ?1 AND group_id = ?2",
                    column
                ),
                params![hash, gid],
                |row| row.get(0),
            )?,
            None => conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM clipboard_items WHERE {} = ?1 AND group_id IS NULL",
                    column
                ),
                params![hash],
                |row| row.get(0),
            )?,
        };
        Ok(count > 0)
    }

    /// 更新已有条目的访问时间并置顶
    pub fn touch_by_hash(
        &self,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.touch_by_column(HashColumn::Content, hash, group_id)
    }

    pub fn touch_by_semantic_hash(
        &self,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.touch_by_column(HashColumn::Semantic, hash, group_id)
    }

    fn touch_by_column(
        &self,
        column: HashColumn,
        hash: &str,
        group_id: Option<i64>,
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

        let (group_cond, group_param) = Self::group_condition(group_id);
        let column = column.as_sql();
        let select_sql = format!(
            "SELECT id FROM clipboard_items \
             WHERE {} = ? AND {} \
             ORDER BY sort_order DESC, created_at DESC, id DESC \
             LIMIT 1",
            column, group_cond
        );

        let target_id: Result<i64, _> = if let Some(gid) = group_param {
            conn.query_row(&select_sql, params![hash, gid], |row| row.get(0))
        } else {
            conn.query_row(&select_sql, params![hash], |row| row.get(0))
        };

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
    pub fn get_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let (group_cond, group_param) = Self::group_condition(group_id);
        let sql = format!(
            "SELECT * FROM clipboard_items \
             WHERE {} \
             ORDER BY is_pinned DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?",
            group_cond
        );
        let result: Result<ClipboardItem, _> = if let Some(gid) = group_param {
            conn.query_row(&sql, params![gid, index as i64], Self::row_to_item)
        } else {
            conn.query_row(&sql, params![index as i64], Self::row_to_item)
        };

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 按收藏列表位置获取完整条目，供收藏快速粘贴使用。
    pub fn get_favorite_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let (group_cond, group_param) = Self::group_condition(group_id);
        let sql = format!(
            "SELECT * FROM clipboard_items \
             WHERE {} AND is_favorite = 1 \
             ORDER BY is_pinned DESC, favorite_order DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?",
            group_cond
        );
        let result: Result<ClipboardItem, _> = if let Some(gid) = group_param {
            conn.query_row(&sql, params![gid, index as i64], Self::row_to_item)
        } else {
            conn.query_row(&sql, params![index as i64], Self::row_to_item)
        };

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// 列表查询列（排除大文本字段以减少 IPC 传输）
    const LIST_COLUMNS: &'static str = "id, content_type, NULL AS text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon";

    /// 搜索查询列（含 text_content 用于关键词上下文预览）
    const SEARCH_COLUMNS: &'static str = "id, content_type, text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon";

    /// 构建通用的 WHERE 条件（content_type / pinned_only / favorite_only / search）
    fn build_filter_conditions(
        options: &QueryOptions,
    ) -> (Vec<String>, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // LIKE 搜索（支持中文，匹配全文任意位置）
        if let Some(ref search) = options.search
            && !search.is_empty()
        {
            conditions.push(
                "(text_content LIKE ? ESCAPE '\\' OR file_paths LIKE ? ESCAPE '\\')".to_string(),
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

        // 分组过滤：None = 默认分组（group_id IS NULL），Some(id) = 自定义分组
        let (group_cond, group_param) = Self::group_condition(options.group_id);
        conditions.push(group_cond.to_string());
        if let Some(gid) = group_param {
            params_vec.push(Box::new(gid));
        }

        (conditions, params_vec)
    }

    /// 将 group_id 转换为 SQL 条件片段和可选参数
    fn group_condition(group_id: Option<i64>) -> (&'static str, Option<i64>) {
        match group_id {
            Some(gid) => ("group_id = ?", Some(gid)),
            None => ("group_id IS NULL", None),
        }
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

        let mut sql = format!("SELECT {} FROM clipboard_items", columns);
        let (conditions, mut params_vec) = Self::build_filter_conditions(&options);
        Self::append_where(&mut sql, &conditions);

        if options.favorite_only {
            sql.push_str(
                " ORDER BY is_pinned DESC, favorite_order DESC, sort_order DESC, created_at DESC",
            );
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
        let was_favorite: bool = conn.query_row(
            "SELECT is_favorite FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        let favorite = !was_favorite;
        let tx = conn.unchecked_transaction()?;

        if favorite {
            let max_favorite_order: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(favorite_order), 0) FROM clipboard_items WHERE is_favorite = 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            tx.execute(
                "UPDATE clipboard_items
                 SET is_favorite = 1, favorite_order = ?1
                 WHERE id = ?2",
                params![max_favorite_order + 1, id],
            )?;
        } else {
            tx.execute(
                "UPDATE clipboard_items
                 SET is_favorite = 0, favorite_order = 0
                 WHERE id = ?1",
                params![id],
            )?;
        }

        tx.commit()?;
        Ok(favorite)
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

    /// 获取可清除条目的图片路径（按分组/类型过滤）
    pub fn get_clearable_image_paths(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .condition("image_path IS NOT NULL")
            .select_strings(&conn, "SELECT image_path FROM clipboard_items", "")
    }

    /// 清空历史（保留置顶和收藏），按分组/类型过滤
    pub fn clear_history(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .delete_items(&conn)
    }

    /// 获取所有条目的图片路径（含置顶和收藏）
    pub fn get_all_image_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt =
            conn.prepare("SELECT image_path FROM clipboard_items WHERE image_path IS NOT NULL")?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// Get all image paths within a specific group (including pinned and favorites).
    pub fn get_image_paths_by_group(&self, group_id: i64) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items \
             WHERE image_path IS NOT NULL AND group_id = ?1",
        )?;
        let paths = stmt
            .query_map(params![group_id], |row| row.get::<_, String>(0))?
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

    /// 删除 N 天前的非置顶/非收藏条目（按分组），返回 (删除数, 关联图片路径)
    pub fn delete_older_than(
        &self,
        days: i64,
        group_id: Option<i64>,
    ) -> Result<(i64, Vec<String>), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let age_cond = "created_at < datetime('now', 'localtime', '-' || ? || ' days')";

        let image_paths = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition("image_path IS NOT NULL")
            .condition_with_param(age_cond, days)
            .select_strings(&conn, "SELECT image_path FROM clipboard_items", "")?;

        let deleted = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition_with_param(age_cond, days)
            .delete_items(&conn)?;

        debug!(
            "Auto-cleanup: deleted {} items older than {} days (group: {:?})",
            deleted, days, group_id
        );
        Ok((deleted, image_paths))
    }

    /// 执行最大数量限制（按分组），返回 (删除数, 图片路径)
    pub fn enforce_max_count(
        &self,
        max_count: i64,
        group_id: Option<i64>,
    ) -> Result<(i64, Vec<String>), rusqlite::Error> {
        if max_count <= 0 {
            return Ok((0, vec![]));
        }

        let conn = self.write_conn.lock();
        let current_count = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .count_items(&conn)?;

        if current_count <= max_count {
            return Ok((0, vec![]));
        }
        let to_delete = current_count - max_count;

        let image_paths = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition("image_path IS NOT NULL")
            .param(to_delete)
            .select_strings(
                &conn,
                "SELECT image_path FROM clipboard_items",
                "ORDER BY created_at ASC LIMIT ?",
            )?;

        let del_cb = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .param(to_delete);
        let delete_sql = format!(
            "DELETE FROM clipboard_items WHERE id IN (\
                SELECT id FROM clipboard_items{} \
                ORDER BY created_at ASC LIMIT ?\
            )",
            del_cb.where_clause()
        );
        let deleted = conn.execute(&delete_sql, del_cb.param_refs().as_slice())? as i64;

        debug!(
            "Enforced max count: deleted {} oldest items (group: {:?})",
            deleted, group_id
        );
        Ok((deleted, image_paths))
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
        let semantic_hash =
            semantic_hash_from_text(new_text).unwrap_or_else(|| content_hash.clone());

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

    /// 交换两个收藏条目的排序位置
    pub fn move_favorite_item_by_id(
        &self,
        from_id: i64,
        to_id: i64,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();

        let from_favorite_order: i64 = conn.query_row(
            "SELECT favorite_order FROM clipboard_items WHERE id = ?1 AND is_favorite = 1",
            params![from_id],
            |row| row.get(0),
        )?;

        let to_favorite_order: i64 = conn.query_row(
            "SELECT favorite_order FROM clipboard_items WHERE id = ?1 AND is_favorite = 1",
            params![to_id],
            |row| row.get(0),
        )?;

        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE clipboard_items SET favorite_order = ?1 WHERE id = ?2 AND is_favorite = 1",
            params![to_favorite_order, from_id],
        )?;
        tx.execute(
            "UPDATE clipboard_items SET favorite_order = ?1 WHERE id = ?2 AND is_favorite = 1",
            params![from_favorite_order, to_id],
        )?;
        tx.commit()?;

        debug!(
            "Moved favorite item {} (favorite_order: {} -> {}) with item {} (favorite_order: {} -> {})",
            from_id,
            from_favorite_order,
            to_favorite_order,
            to_id,
            to_favorite_order,
            from_favorite_order
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
            favorite_order: row.get("favorite_order")?,
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

    /// 导入同步条目（基于 content_hash 去重，已存在则跳过）
    pub fn import_sync_items(&self, items: &[ClipboardItem]) -> Result<usize, rusqlite::Error> {
        let mut conn = self.write_conn.lock();
        let mut count = 0usize;

        let tx = conn.transaction()?;
        {
            let mut exists_stmt =
                tx.prepare_cached("SELECT 1 FROM clipboard_items WHERE content_hash = ?1 LIMIT 1")?;
            let mut insert_stmt = tx.prepare_cached(
                "INSERT INTO clipboard_items
                 (content_type, text_content, html_content, rtf_content, image_path, file_paths,
                  content_hash, semantic_hash, preview, byte_size, image_width, image_height,
                  is_pinned, is_favorite, sort_order, created_at, updated_at,
                  access_count, last_accessed_at, char_count, source_app_name, source_app_icon)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)"
            )?;

            for item in items {
                let exists = exists_stmt.exists(params![item.content_hash])?;
                if exists {
                    continue;
                }
                insert_stmt.execute(params![
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
                    item.sort_order,
                    item.created_at,
                    item.updated_at,
                    item.access_count,
                    item.last_accessed_at,
                    item.char_count,
                    item.source_app_name,
                    item.source_app_icon,
                ])?;
                count += 1;
            }

            if count > 0 {
                Self::rebuild_sort_order_by_created_at(&tx)?;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    fn rebuild_sort_order_by_created_at(tx: &Transaction<'_>) -> Result<(), rusqlite::Error> {
        let ids: Vec<i64> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM clipboard_items
                 ORDER BY
                   CASE
                     WHEN created_at IS NULL OR trim(created_at) = '' OR datetime(created_at) IS NULL THEN 0
                     ELSE 1
                   END ASC,
                   datetime(created_at) ASC,
                   id ASC",
            )?;
            stmt.query_map([], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut update_stmt =
            tx.prepare_cached("UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2")?;
        for (index, id) in ids.iter().enumerate() {
            update_stmt.execute(params![index as i64 + 1, id])?;
        }
        Ok(())
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

    /// 批量读取多个设置项，单次查询
    pub fn get_batch(&self, keys: &[&str]) -> std::collections::HashMap<String, Option<String>> {
        let conn = self.read_conn.lock();
        let mut result = std::collections::HashMap::new();
        let placeholders: Vec<String> = keys
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT key, value FROM settings WHERE key IN ({})",
            placeholders.join(", ")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = keys
            .iter()
            .map(|k| k as &dyn rusqlite::types::ToSql)
            .collect();

        if let Ok(mut stmt) = conn.prepare(&sql) {
            let rows = stmt.query_map(params.as_slice(), |row| {
                let key: String = row.get(0)?;
                let value: Option<String> = row.get(1)?;
                Ok((key, value))
            });

            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    result.insert(row.0, row.1);
                }
            }
        }

        // 确保所有请求的 key 都有条目
        for key in keys {
            result.entry(key.to_string()).or_insert(None);
        }
        result
    }

    /// 读取字符串设置，缺失或出错时返回 default。
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key)
            .ok()
            .flatten()
            .unwrap_or_else(|| default.to_string())
    }

    /// 读取布尔设置（"true"/"false"），缺失或出错时返回 default。
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key)
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(default)
    }

    /// 读取并解析为指定类型，缺失/出错/解析失败时返回 None。
    pub fn get_parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key).ok().flatten().and_then(|v| v.parse().ok())
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

    /// 批量获取指定 key 的设置值，缺失的 key 不包含在结果中
    pub fn get_multiple(
        &self,
        keys: &[&str],
    ) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        if keys.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let conn = self.read_conn.lock();
        let placeholders = keys
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT key, value FROM settings WHERE key IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let map = stmt
            .query_map(rusqlite::params_from_iter(keys.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(map)
    }

    /// 清空所有设置
    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM settings", [])?;
        Ok(())
    }
}

/// 自定义分组仓库
pub struct GroupRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl GroupRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// 列出所有分组（含每个分组的条目数）
    pub fn list_with_count(&self) -> Result<Vec<Group>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT g.id, g.name, g.color, g.sort_order, g.created_at, \
             COUNT(ci.id) AS item_count \
             FROM groups g \
             LEFT JOIN clipboard_items ci ON ci.group_id = g.id \
             GROUP BY g.id \
             ORDER BY g.sort_order ASC, g.created_at ASC",
        )?;
        let groups = stmt
            .query_map([], |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    sort_order: row.get(3)?,
                    created_at: row.get(4)?,
                    item_count: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(groups)
    }

    /// 创建新分组，返回完整分组对象
    pub fn create(&self, name: &str, color: Option<&str>) -> Result<Group, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let max_sort: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM groups",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        conn.execute(
            "INSERT INTO groups (name, color, sort_order) VALUES (?1, ?2, ?3)",
            params![name, color, max_sort + 1],
        )?;
        let id = conn.last_insert_rowid();
        let group = conn.query_row(
            "SELECT g.id, g.name, g.color, g.sort_order, g.created_at, 0 AS item_count FROM groups g WHERE g.id = ?1",
            params![id],
            |row| Ok(Group {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                sort_order: row.get(3)?,
                created_at: row.get(4)?,
                item_count: row.get(5)?,
            }),
        )?;
        debug!("Created group: id={}, name={}", id, name);
        Ok(group)
    }

    /// 重命名分组
    pub fn rename(&self, id: i64, name: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE groups SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        debug!("Renamed group {} to {}", id, name);
        Ok(())
    }

    /// 更新分组颜色
    pub fn update_color(&self, id: i64, color: Option<&str>) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE groups SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
        debug!("Updated color of group {} to {:?}", id, color);
        Ok(())
    }

    /// 删除分组（ON DELETE CASCADE 自动删除该分组的所有 clipboard_items）
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        debug!("Deleted group {}", id);
        Ok(())
    }

    /// 删除所有自定义分组（及其关联条目通过 ON DELETE CASCADE 一并删除）
    pub fn delete_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM groups", [])?;
        debug!("Deleted all groups");
        Ok(())
    }

    /// 将条目移动到指定分组（None = 移回默认分组）
    pub fn move_item_to_group(
        &self,
        item_id: i64,
        group_id: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET group_id = ?1 WHERE id = ?2",
            params![group_id, item_id],
        )?;
        debug!("Moved item {} to group {:?}", item_id, group_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    fn temp_db() -> Database {
        let dir = std::env::temp_dir().join(format!("ec_repo_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "test_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        Database::new(path).unwrap()
    }

    fn make_text_item(text: &str) -> NewClipboardItem {
        let hash = blake3::hash(format!("text:{}", text).as_bytes())
            .to_hex()
            .to_string();
        NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(text.to_string()),
            preview: Some(text.chars().take(200).collect()),
            content_hash: hash.clone(),
            semantic_hash: hash,
            byte_size: text.len() as i64,
            char_count: Some(text.chars().count() as i64),
            ..Default::default()
        }
    }

    // ==================== ClipboardRepository ====================

    #[test]
    fn insert_and_get_by_id() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("hello")).unwrap();
        let item = repo.get_by_id(id).unwrap().unwrap();
        assert_eq!(item.text_content.as_deref(), Some("hello"));
        assert_eq!(item.content_type, "text");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        assert!(repo.get_by_id(999).unwrap().is_none());
    }

    #[test]
    fn insert_increments_sort_order() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id1 = repo.insert(make_text_item("first")).unwrap();
        let id2 = repo.insert(make_text_item("second")).unwrap();
        let item1 = repo.get_by_id(id1).unwrap().unwrap();
        let item2 = repo.get_by_id(id2).unwrap().unwrap();
        assert!(item2.sort_order > item1.sort_order);
    }

    #[test]
    fn exists_by_hash() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let item = make_text_item("test_exists");
        let hash = item.content_hash.clone();
        repo.insert(item).unwrap();
        assert!(repo.exists_by_hash(&hash, None).unwrap());
        assert!(!repo.exists_by_hash("nonexistent", None).unwrap());
    }

    #[test]
    fn exists_by_hash_respects_group() {
        let db = temp_db();
        let group_repo = GroupRepository::new(&db);
        let group = group_repo.create("test_group", None).unwrap();

        let repo = ClipboardRepository::new(&db);
        let mut item = make_text_item("grouped");
        item.group_id = Some(group.id);
        let hash = item.content_hash.clone();
        repo.insert(item).unwrap();

        assert!(!repo.exists_by_hash(&hash, None).unwrap());
        assert!(repo.exists_by_hash(&hash, Some(group.id)).unwrap());
    }

    #[test]
    fn touch_by_hash_updates_sort_order() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let item = make_text_item("touchable");
        let hash = item.content_hash.clone();
        let id = repo.insert(item).unwrap();
        let original = repo.get_by_id(id).unwrap().unwrap();

        repo.insert(make_text_item("spacer")).unwrap();
        let touched_id = repo.touch_by_hash(&hash, None).unwrap();
        assert_eq!(touched_id, Some(id));

        let updated = repo.get_by_id(id).unwrap().unwrap();
        assert!(updated.sort_order > original.sort_order);
        assert!(updated.access_count > original.access_count);
    }

    #[test]
    fn touch_nonexistent_returns_none() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        assert!(repo.touch_by_hash("no_such_hash", None).unwrap().is_none());
    }

    #[test]
    fn delete_item() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("to_delete")).unwrap();
        assert!(repo.get_by_id(id).unwrap().is_some());
        repo.delete(id).unwrap();
        assert!(repo.get_by_id(id).unwrap().is_none());
    }

    #[test]
    fn batch_delete() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id1 = repo.insert(make_text_item("batch1")).unwrap();
        let id2 = repo.insert(make_text_item("batch2")).unwrap();
        let id3 = repo.insert(make_text_item("batch3")).unwrap();

        let (deleted, _paths) = repo.batch_delete(&[id1, id3]).unwrap();
        assert_eq!(deleted, 2);
        assert!(repo.get_by_id(id1).unwrap().is_none());
        assert!(repo.get_by_id(id2).unwrap().is_some());
        assert!(repo.get_by_id(id3).unwrap().is_none());
    }

    #[test]
    fn batch_delete_empty_ids() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let (deleted, paths) = repo.batch_delete(&[]).unwrap();
        assert_eq!(deleted, 0);
        assert!(paths.is_empty());
    }

    #[test]
    fn toggle_pin() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("pin_test")).unwrap();
        assert!(!repo.get_by_id(id).unwrap().unwrap().is_pinned);
        let pinned = repo.toggle_pin(id).unwrap();
        assert!(pinned);
        assert!(repo.get_by_id(id).unwrap().unwrap().is_pinned);
        let unpinned = repo.toggle_pin(id).unwrap();
        assert!(!unpinned);
    }

    #[test]
    fn toggle_favorite() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("fav_test")).unwrap();
        let fav = repo.toggle_favorite(id).unwrap();
        assert!(fav);
        let item = repo.get_by_id(id).unwrap().unwrap();
        assert!(item.is_favorite);
        assert!(item.favorite_order > 0);

        let unfav = repo.toggle_favorite(id).unwrap();
        assert!(!unfav);
        let item = repo.get_by_id(id).unwrap().unwrap();
        assert!(!item.is_favorite);
        assert_eq!(item.favorite_order, 0);
    }

    #[test]
    fn list_default_ordering() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("older")).unwrap();
        repo.insert(make_text_item("newer")).unwrap();

        let items = repo.list(QueryOptions::default()).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].sort_order > items[1].sort_order);
    }

    #[test]
    fn list_with_search() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("hello world")).unwrap();
        repo.insert(make_text_item("goodbye world")).unwrap();
        repo.insert(make_text_item("hello rust")).unwrap();

        let items = repo
            .list(QueryOptions {
                search: Some("hello".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn list_with_content_type_filter() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("text item")).unwrap();

        let img_item = NewClipboardItem {
            content_type: ContentType::Image,
            content_hash: "img_hash".to_string(),
            semantic_hash: "img_hash".to_string(),
            image_path: Some("/fake/path.png".to_string()),
            ..Default::default()
        };
        repo.insert(img_item).unwrap();

        let text_only = repo
            .list(QueryOptions {
                content_type: Some("text".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(text_only.len(), 1);
    }

    #[test]
    fn list_with_limit_and_offset() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        for i in 0..5 {
            repo.insert(make_text_item(&format!("item_{}", i))).unwrap();
        }

        let items = repo
            .list(QueryOptions {
                limit: Some(2),
                offset: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn count_items() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        for i in 0..3 {
            repo.insert(make_text_item(&format!("count_{}", i))).unwrap();
        }
        let count = repo.count(QueryOptions::default()).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn count_with_filter() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("fav count")).unwrap();
        let id2 = repo.insert(make_text_item("fav count 2")).unwrap();
        repo.toggle_favorite(id2).unwrap();

        let fav_count = repo
            .count(QueryOptions {
                favorite_only: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(fav_count, 1);
    }

    #[test]
    fn clear_history_preserves_pinned_and_favorites() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id1 = repo.insert(make_text_item("normal")).unwrap();
        let id2 = repo.insert(make_text_item("pinned")).unwrap();
        let id3 = repo.insert(make_text_item("favorite")).unwrap();
        repo.toggle_pin(id2).unwrap();
        repo.toggle_favorite(id3).unwrap();

        let deleted = repo.clear_history(None, None).unwrap();
        assert_eq!(deleted, 1);
        assert!(repo.get_by_id(id1).unwrap().is_none());
        assert!(repo.get_by_id(id2).unwrap().is_some());
        assert!(repo.get_by_id(id3).unwrap().is_some());
    }

    #[test]
    fn clear_all() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("all1")).unwrap();
        let id2 = repo.insert(make_text_item("all2")).unwrap();
        repo.toggle_pin(id2).unwrap();
        repo.clear_all().unwrap();
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 0);
    }

    #[test]
    fn move_item_by_id_swaps_sort_order() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id1 = repo.insert(make_text_item("move_a")).unwrap();
        let id2 = repo.insert(make_text_item("move_b")).unwrap();
        let sort1 = repo.get_by_id(id1).unwrap().unwrap().sort_order;
        let sort2 = repo.get_by_id(id2).unwrap().unwrap().sort_order;

        repo.move_item_by_id(id1, id2).unwrap();
        assert_eq!(repo.get_by_id(id1).unwrap().unwrap().sort_order, sort2);
        assert_eq!(repo.get_by_id(id2).unwrap().unwrap().sort_order, sort1);
    }

    #[test]
    fn bump_to_top() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id1 = repo.insert(make_text_item("bump1")).unwrap();
        let id2 = repo.insert(make_text_item("bump2")).unwrap();
        let sort2 = repo.get_by_id(id2).unwrap().unwrap().sort_order;

        repo.bump_to_top(id1).unwrap();
        let new_sort1 = repo.get_by_id(id1).unwrap().unwrap().sort_order;
        assert!(new_sort1 > sort2);
    }

    #[test]
    fn bump_to_top_skips_pinned() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("pinned_bump")).unwrap();
        repo.toggle_pin(id).unwrap();
        let original_sort = repo.get_by_id(id).unwrap().unwrap().sort_order;
        repo.bump_to_top(id).unwrap();
        assert_eq!(repo.get_by_id(id).unwrap().unwrap().sort_order, original_sort);
    }

    #[test]
    fn update_text_content() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("original text")).unwrap();
        repo.update_text_content(id, "updated text").unwrap();
        let item = repo.get_by_id(id).unwrap().unwrap();
        assert_eq!(item.text_content.as_deref(), Some("updated text"));
        assert_eq!(item.content_type, "text");
        assert_eq!(item.char_count, Some(12));
    }

    #[test]
    fn get_by_position() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("pos0")).unwrap();
        repo.insert(make_text_item("pos1")).unwrap();

        let first = repo.get_by_position(0, None).unwrap().unwrap();
        assert_eq!(first.preview.as_deref(), Some("pos1"));
        let second = repo.get_by_position(1, None).unwrap().unwrap();
        assert_eq!(second.preview.as_deref(), Some("pos0"));
    }

    #[test]
    fn enforce_max_count() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        for i in 0..5 {
            repo.insert(make_text_item(&format!("enforce_{}", i))).unwrap();
        }
        let (deleted, _) = repo.enforce_max_count(3, None).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(
            repo.count(QueryOptions::default()).unwrap(),
            3
        );
    }

    #[test]
    fn enforce_max_count_no_op_when_under_limit() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("under_limit")).unwrap();
        let (deleted, _) = repo.enforce_max_count(10, None).unwrap();
        assert_eq!(deleted, 0);
    }

    // ==================== SettingsRepository ====================

    #[test]
    fn settings_get_set() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("my_key", "my_value").unwrap();
        assert_eq!(repo.get("my_key").unwrap(), Some("my_value".to_string()));
    }

    #[test]
    fn settings_get_nonexistent() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        assert_eq!(repo.get("no_such_key").unwrap(), None);
    }

    #[test]
    fn settings_get_or_default() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        assert_eq!(repo.get_or("missing", "fallback"), "fallback");
        repo.set("present", "value").unwrap();
        assert_eq!(repo.get_or("present", "fallback"), "value");
    }

    #[test]
    fn settings_get_bool() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("flag_true", "true").unwrap();
        repo.set("flag_false", "false").unwrap();
        assert!(repo.get_bool("flag_true", false));
        assert!(!repo.get_bool("flag_false", true));
        assert!(repo.get_bool("missing_flag", true));
    }

    #[test]
    fn settings_get_parsed() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("num", "42").unwrap();
        assert_eq!(repo.get_parsed::<i32>("num"), Some(42));
        assert_eq!(repo.get_parsed::<i32>("missing"), None);
    }

    #[test]
    fn settings_get_batch() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("k1", "v1").unwrap();
        repo.set("k2", "v2").unwrap();
        let result = repo.get_batch(&["k1", "k2", "k3"]);
        assert_eq!(result.get("k1").unwrap(), &Some("v1".to_string()));
        assert_eq!(result.get("k2").unwrap(), &Some("v2".to_string()));
        assert_eq!(result.get("k3").unwrap(), &None);
    }

    #[test]
    fn settings_get_multiple() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("a", "1").unwrap();
        repo.set("b", "2").unwrap();
        let result = repo.get_multiple(&["a", "b", "c"]).unwrap();
        assert_eq!(result.get("a").unwrap(), "1");
        assert_eq!(result.get("b").unwrap(), "2");
        assert!(!result.contains_key("c"));
    }

    #[test]
    fn settings_get_all() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        let all = repo.get_all().unwrap();
        assert!(all.contains_key("global_shortcut"));
    }

    #[test]
    fn settings_clear_all() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("extra", "val").unwrap();
        repo.clear_all().unwrap();
        assert!(repo.get_all().unwrap().is_empty());
    }

    #[test]
    fn settings_upsert() {
        let db = temp_db();
        let repo = SettingsRepository::new(&db);
        repo.set("upsert_key", "first").unwrap();
        repo.set("upsert_key", "second").unwrap();
        assert_eq!(repo.get("upsert_key").unwrap(), Some("second".to_string()));
    }

    // ==================== GroupRepository ====================

    #[test]
    fn group_create_and_list() {
        let db = temp_db();
        let repo = GroupRepository::new(&db);
        let group = repo.create("Test Group", Some("#ff0000")).unwrap();
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.color.as_deref(), Some("#ff0000"));
        assert_eq!(group.item_count, 0);

        let groups = repo.list_with_count().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Test Group");
    }

    #[test]
    fn group_rename() {
        let db = temp_db();
        let repo = GroupRepository::new(&db);
        let group = repo.create("Old Name", None).unwrap();
        repo.rename(group.id, "New Name").unwrap();
        let groups = repo.list_with_count().unwrap();
        assert_eq!(groups[0].name, "New Name");
    }

    #[test]
    fn group_update_color() {
        let db = temp_db();
        let repo = GroupRepository::new(&db);
        let group = repo.create("Colored", None).unwrap();
        assert!(group.color.is_none());
        repo.update_color(group.id, Some("#00ff00")).unwrap();
        let groups = repo.list_with_count().unwrap();
        assert_eq!(groups[0].color.as_deref(), Some("#00ff00"));
    }

    #[test]
    fn group_delete() {
        let db = temp_db();
        let repo = GroupRepository::new(&db);
        let group = repo.create("To Delete", None).unwrap();
        repo.delete(group.id).unwrap();
        assert!(repo.list_with_count().unwrap().is_empty());
    }

    #[test]
    fn group_delete_cascades_items() {
        let db = temp_db();
        let group_repo = GroupRepository::new(&db);
        let group = group_repo.create("Cascade Group", None).unwrap();

        let clip_repo = ClipboardRepository::new(&db);
        let mut item = make_text_item("grouped_item");
        item.group_id = Some(group.id);
        let id = clip_repo.insert(item).unwrap();

        group_repo.delete(group.id).unwrap();
        assert!(clip_repo.get_by_id(id).unwrap().is_none());
    }

    #[test]
    fn group_move_item() {
        let db = temp_db();
        let group_repo = GroupRepository::new(&db);
        let group = group_repo.create("Move Target", None).unwrap();

        let clip_repo = ClipboardRepository::new(&db);
        let id = clip_repo.insert(make_text_item("movable")).unwrap();

        group_repo.move_item_to_group(id, Some(group.id)).unwrap();
        assert!(clip_repo.exists_by_hash(
            &clip_repo.get_by_id(id).unwrap().unwrap().content_hash,
            Some(group.id),
        ).unwrap());

        group_repo.move_item_to_group(id, None).unwrap();
        assert!(clip_repo.exists_by_hash(
            &clip_repo.get_by_id(id).unwrap().unwrap().content_hash,
            None,
        ).unwrap());
    }

    #[test]
    fn group_delete_all() {
        let db = temp_db();
        let repo = GroupRepository::new(&db);
        repo.create("G1", None).unwrap();
        repo.create("G2", None).unwrap();
        repo.delete_all().unwrap();
        assert!(repo.list_with_count().unwrap().is_empty());
    }

    #[test]
    fn group_item_count() {
        let db = temp_db();
        let group_repo = GroupRepository::new(&db);
        let group = group_repo.create("Counting", None).unwrap();

        let clip_repo = ClipboardRepository::new(&db);
        let mut item1 = make_text_item("count_a");
        item1.group_id = Some(group.id);
        let mut item2 = make_text_item("count_b");
        item2.group_id = Some(group.id);
        clip_repo.insert(item1).unwrap();
        clip_repo.insert(item2).unwrap();

        let groups = group_repo.list_with_count().unwrap();
        assert_eq!(groups[0].item_count, 2);
    }
}
