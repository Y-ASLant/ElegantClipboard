use super::{ContentType, Database};
use parking_lot::Mutex;
use rusqlite::{Connection, Row, params};
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
    /// 文件剪贴板 fidelity payload（CF_HDROP + 伴生格式 + staging）
    pub file_payload: Option<String>,
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
    /// 所属分组（NULL = 默认分组，Some(id) = 自定义分组）
    pub group_id: Option<i64>,
    /// 文件是否有效（查询时计算，不存储）
    #[serde(default, skip_deserializing)]
    pub files_valid: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub file_payload: Option<String>,
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
        self.params
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect()
    }

    /// SELECT single-column string results.
    fn select_strings(
        &self,
        conn: &Connection,
        prefix: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let sql = format!("{}{}", prefix, self.where_clause());
        let refs = self.param_refs();
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(refs.as_slice(), |row| row.get::<_, String>(0))?
            .collect()
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
            .as_ref()
            .map(|paths| serde_json::to_string(paths).unwrap_or_default());

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
             (content_type, text_content, html_content, rtf_content, image_path, file_paths, file_payload,
              content_hash, semantic_hash, preview, byte_size, image_width, image_height, sort_order,
              char_count, source_app_name, source_app_icon, group_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                item.content_type.as_str(),
                item.text_content,
                item.html_content,
                item.rtf_content,
                item.image_path,
                file_paths_json,
                item.file_payload,
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

    /// 更新已有条目的访问时间并置顶
    pub fn touch_by_hash(
        &self,
        hash: &str,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        // 注意：write_conn 是单连接 + Mutex，SELECT MAX 和 UPDATE 在同一锁作用域内，
        // 已经是串行安全的，无需额外事务保护。
        let conn = self.write_conn.lock();

        let (group_cond, group_param) = Self::group_condition(group_id);
        let select_sql = format!(
            "SELECT id FROM clipboard_items \
             WHERE content_hash = ? AND {group_cond} \
             ORDER BY sort_order DESC, created_at DESC, id DESC \
             LIMIT 1"
        );

        let target_id: Result<i64, _> = if let Some(gid) = group_param {
            conn.query_row(&select_sql, params![hash, gid], |row| row.get(0))
        } else {
            conn.query_row(&select_sql, params![hash], |row| row.get(0))
        };

        match target_id {
            Ok(id) => {
                let max_sort_order: i64 = conn
                    .query_row(
                        "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                let new_sort = max_sort_order + 1;

                conn.execute(
                    "UPDATE clipboard_items \
                     SET access_count = access_count + 1, \
                         last_accessed_at = datetime('now', 'localtime'), \
                         updated_at = datetime('now', 'localtime'), \
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

    pub fn set_source_app(
        &self,
        id: i64,
        name: &str,
        icon: Option<&str>,
    ) -> Result<bool, rusqlite::Error> {
        Ok(self.write_conn.lock().execute(
            "UPDATE clipboard_items SET source_app_name = ?1, source_app_icon = ?2 WHERE id = ?3",
            params![name, icon, id],
        )? > 0)
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
             WHERE {group_cond} \
             ORDER BY is_pinned DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?"
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
             WHERE {group_cond} AND is_favorite = 1 \
             ORDER BY is_pinned DESC, favorite_order DESC, sort_order DESC, created_at DESC \
             LIMIT 1 OFFSET ?"
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
         image_path, file_paths, NULL AS file_payload, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, group_id";

    /// 搜索查询列（含 text_content 用于关键词上下文预览）
    const SEARCH_COLUMNS: &'static str = "id, content_type, text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, NULL AS file_payload, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, group_id";

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
                "(text_content LIKE ?1 ESCAPE '\\' OR file_paths LIKE ?1 ESCAPE '\\')".to_string(),
            );
            let mut pattern = String::with_capacity(search.len() + 2);
            pattern.push('%');
            for ch in search.chars() {
                if matches!(ch, '\\' | '%' | '_') {
                    pattern.push('\\');
                }
                pattern.push(ch);
            }
            pattern.push('%');
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
            .map(str::trim)
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

        let is_searching = options.search.as_ref().is_some_and(|s| !s.is_empty());
        let columns = if is_searching {
            Self::SEARCH_COLUMNS
        } else {
            Self::LIST_COLUMNS
        };

        let mut sql = format!("SELECT {columns} FROM clipboard_items");
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
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params_refs.as_slice(), Self::row_to_item)?
            .collect()
    }

    pub fn count(&self, options: QueryOptions) -> Result<i64, rusqlite::Error> {
        let conn = self.read_conn.lock();

        let mut sql = "SELECT COUNT(*) FROM clipboard_items".to_string();
        let (conditions, params_vec) = Self::build_filter_conditions(&options);
        Self::append_where(&mut sql, &conditions);

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    /// Counts persisted items by local calendar date, starting inclusively at `start_date`.
    /// Dates with no items are omitted.
    pub fn daily_counts(&self, start_date: &str) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT date(created_at), COUNT(*) FROM clipboard_items \
             WHERE created_at >= ?1 \
             GROUP BY date(created_at) \
             ORDER BY date(created_at) ASC",
        )?;
        stmt.query_map(params![start_date], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
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
            .select_strings(&conn, "SELECT image_path FROM clipboard_items")
    }

    /// 获取可清除条目的 file_payload（按分组/类型过滤）
    pub fn get_clearable_file_payloads(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .condition("file_payload IS NOT NULL")
            .select_strings(&conn, "SELECT file_payload FROM clipboard_items")
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
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
    }

    /// 获取所有条目的 file_payload（含置顶和收藏）
    pub fn get_all_file_payloads(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn
            .prepare("SELECT file_payload FROM clipboard_items WHERE file_payload IS NOT NULL")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
    }

    /// 清空所有历史（包括置顶和收藏）
    pub fn clear_all(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute("DELETE FROM clipboard_items", [])?;
        Ok(deleted as i64)
    }

    /// 删除 N 天前的非置顶/非收藏条目（按分组），返回 (删除数, 图片路径, file_payload)
    pub fn delete_older_than(
        &self,
        days: i64,
        group_id: Option<i64>,
    ) -> Result<(i64, Vec<String>, Vec<String>), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let age_cond = "created_at < datetime('now', 'localtime', '-' || ? || ' days')";

        let image_paths = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition("image_path IS NOT NULL")
            .condition_with_param(age_cond, days)
            .select_strings(&conn, "SELECT image_path FROM clipboard_items")?;

        let file_payloads = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition("file_payload IS NOT NULL")
            .condition_with_param(age_cond, days)
            .select_strings(&conn, "SELECT file_payload FROM clipboard_items")?;

        let deleted = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .condition_with_param(age_cond, days)
            .delete_items(&conn)?;

        debug!(
            "Auto-cleanup: deleted {} items older than {} days (group: {:?})",
            deleted, days, group_id
        );
        Ok((deleted, image_paths, file_payloads))
    }

    /// 执行最大数量限制（按分组），返回 (删除数, 图片路径, file_payload)
    pub fn enforce_max_count(
        &self,
        max_count: i64,
        group_id: Option<i64>,
    ) -> Result<(i64, Vec<String>, Vec<String>), rusqlite::Error> {
        if max_count <= 0 {
            return Ok((0, vec![], vec![]));
        }

        let conn = self.write_conn.lock();
        let current_count = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .count_items(&conn)?;

        if current_count <= max_count {
            return Ok((0, vec![], vec![]));
        }
        let to_delete = current_count - max_count;

        let candidates = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .param(to_delete);
        let selector = format!(
            "SELECT id FROM clipboard_items{} ORDER BY created_at ASC, id ASC LIMIT ?",
            candidates.where_clause()
        );
        let params = candidates.param_refs();
        let mut image_paths = Vec::new();
        let mut file_payloads = Vec::new();
        {
            let sql = format!(
                "SELECT image_path, file_payload FROM clipboard_items WHERE id IN ({selector})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })?;
            for row in rows {
                let (image, payload) = row?;
                image_paths.extend(image);
                file_payloads.extend(payload);
            }
        }
        let deleted = conn.execute(
            &format!("DELETE FROM clipboard_items WHERE id IN ({selector})"),
            params.as_slice(),
        )? as i64;

        debug!(
            "Enforced max count: deleted {} oldest items (group: {:?})",
            deleted, group_id
        );
        Ok((deleted, image_paths, file_payloads))
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

    fn row_to_item(row: &Row) -> Result<ClipboardItem, rusqlite::Error> {
        Ok(ClipboardItem {
            id: row.get("id")?,
            content_type: row.get("content_type")?,
            text_content: row.get("text_content")?,
            html_content: row.get("html_content")?,
            rtf_content: row.get("rtf_content")?,
            image_path: row.get("image_path")?,
            file_paths: row.get("file_paths")?,
            file_payload: row.get("file_payload")?,
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
            group_id: row.get("group_id")?,
            files_valid: None, // 查询时计算
        })
    }

    pub fn update_item_media_paths(
        &self,
        id: i64,
        image_path: Option<&str>,
        file_payload: Option<&str>,
        source_app_icon: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET image_path = ?1, file_payload = ?2, source_app_icon = ?3 \
             WHERE id = ?4",
            params![image_path, file_payload, source_app_icon, id],
        )?;
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

    pub fn set(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_batch(&self, values: &[(&str, &str)]) -> Result<(), rusqlite::Error> {
        let mut conn = self.write_conn.lock();
        let tx = conn.transaction()?;
        for (key, value) in values {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
                params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_all(&self) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect()
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
            .collect::<Result<Vec<_>, _>>()?;
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

    /// 删除分组（ON DELETE CASCADE 自动删除该分组的所有 clipboard_items）
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        debug!("Deleted group {}", id);
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
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("ec_repo_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!(
            "test_{}_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n
        ));
        Database::new(path).unwrap()
    }

    fn make_text_item(text: &str) -> NewClipboardItem {
        let hash = blake3::hash(format!("text:{text}").as_bytes())
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
    fn list_reports_invalid_row_instead_of_silently_skipping_it() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let valid_id = repo.insert(make_text_item("valid")).unwrap();
        let invalid_id = repo.insert(make_text_item("invalid")).unwrap();

        db.write_connection()
            .lock()
            .execute(
                "UPDATE clipboard_items SET byte_size = X'01' WHERE id = ?1",
                params![invalid_id],
            )
            .unwrap();

        assert!(repo.list(QueryOptions::default()).is_err());
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 2);
        assert_eq!(
            repo.get_by_id(valid_id)
                .unwrap()
                .unwrap()
                .text_content
                .as_deref(),
            Some("valid")
        );
    }

    #[test]
    fn media_reference_queries_fail_if_a_path_cannot_be_read() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let id = repo.insert(make_text_item("media reference")).unwrap();
        db.write_connection()
            .lock()
            .execute(
                "UPDATE clipboard_items SET image_path = X'01', file_payload = X'01' WHERE id = ?1",
                params![id],
            )
            .unwrap();

        assert!(repo.get_all_image_paths().is_err());
        assert!(repo.get_all_file_payloads().is_err());
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 1);
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
    fn search_reuses_escaped_pattern_with_filters_and_pagination() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::new(directory.path().join("clipboard.db")).unwrap();
        let repo = ClipboardRepository::new(&db);
        let group_id = GroupRepository::new(&db).create("search", None).unwrap().id;
        let mut text = make_text_item(r"中文%_\text");
        text.group_id = Some(group_id);
        repo.insert(text).unwrap();
        repo.insert(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(vec![r"C:\中文%_\file".into()]),
            content_hash: "search-files".into(),
            semantic_hash: "search-files".into(),
            group_id: Some(group_id),
            ..Default::default()
        })
        .unwrap();
        repo.insert(make_text_item(r"中文%_\other group")).unwrap();
        let mut nonmatch = make_text_item("中文ab");
        nonmatch.group_id = Some(group_id);
        repo.insert(nonmatch).unwrap();

        for search in ["%", "_", "\\", "中文%_"] {
            let options = QueryOptions {
                search: Some(search.into()),
                content_type: Some("text,files".into()),
                group_id: Some(group_id),
                limit: Some(1),
                offset: Some(0),
                ..Default::default()
            };
            assert_eq!(repo.count(options.clone()).unwrap(), 2);
            let first = repo.list(options.clone()).unwrap();
            let second = repo
                .list(QueryOptions {
                    offset: Some(1),
                    ..options
                })
                .unwrap();
            assert_eq!(first.len(), 1);
            assert_eq!(second.len(), 1);
            assert_ne!(first[0].id, second[0].id);
        }
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
            repo.insert(make_text_item(&format!("item_{i}"))).unwrap();
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
            repo.insert(make_text_item(&format!("count_{i}"))).unwrap();
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
    fn daily_counts_groups_all_items_from_inclusive_start_date() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let group = GroupRepository::new(&db)
            .create("Daily counts group", None)
            .unwrap();

        let entries = [
            ("before boundary", "2026-09-19 23:59:59", false),
            ("at boundary", "2026-09-20 00:00:00", false),
            ("same day grouped", "2026-09-20 12:34:56", true),
            ("same day ungrouped", "2026-09-20 23:59:59", false),
            ("later grouped", "2026-09-22 00:00:00", true),
            ("later deleted", "2026-09-22 16:00:00", false),
            ("latest", "2026-09-23 23:59:59", false),
        ];
        let mut deleted_id = None;
        for (label, timestamp, grouped) in entries {
            let mut item = make_text_item(label);
            if grouped {
                item.group_id = Some(group.id);
            }
            let id = repo.insert(item).unwrap();
            db.write_connection()
                .lock()
                .execute(
                    "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
                    params![timestamp, id],
                )
                .unwrap();
            if label == "later deleted" {
                deleted_id = Some(id);
            }
        }
        repo.delete(deleted_id.unwrap()).unwrap();

        assert_eq!(
            repo.daily_counts("2026-09-20").unwrap(),
            vec![
                ("2026-09-20".to_string(), 3),
                ("2026-09-22".to_string(), 1),
                ("2026-09-23".to_string(), 1),
            ]
        );
        assert_eq!(
            repo.daily_counts("2026-09-22").unwrap(),
            vec![("2026-09-22".to_string(), 1), ("2026-09-23".to_string(), 1),]
        );
        assert!(repo.daily_counts("2026-09-24").unwrap().is_empty());
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
        assert_eq!(
            repo.get_by_id(id).unwrap().unwrap().sort_order,
            original_sort
        );
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
            repo.insert(make_text_item(&format!("enforce_{i}")))
                .unwrap();
        }
        let (deleted, _, _) = repo.enforce_max_count(3, None).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(repo.count(QueryOptions::default()).unwrap(), 3);
    }

    #[test]
    fn enforce_max_count_no_op_when_under_limit() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        repo.insert(make_text_item("under_limit")).unwrap();
        let (deleted, _, _) = repo.enforce_max_count(10, None).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn enforce_max_count_reports_only_media_from_evicted_rows() {
        let db = temp_db();
        let repo = ClipboardRepository::new(&db);
        let text = repo.insert(make_text_item("old text")).unwrap();
        let image = repo.insert(make_text_item("old image")).unwrap();
        let files = repo.insert(make_text_item("recent files")).unwrap();
        {
            let connection = db.write_connection();
            let conn = connection.lock();
            for (id, timestamp) in [
                (text, "2020-01-01 00:00:00"),
                (image, "2020-01-02 00:00:00"),
                (files, "2020-01-03 00:00:00"),
            ] {
                conn.execute(
                    "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
                    params![timestamp, id],
                )
                .unwrap();
            }
            conn.execute(
                "UPDATE clipboard_items SET image_path = 'evicted.png' WHERE id = ?1",
                [image],
            )
            .unwrap();
            conn.execute(
                "UPDATE clipboard_items SET file_payload = 'retained.json' WHERE id = ?1",
                [files],
            )
            .unwrap();
        }

        let (deleted, images, payloads) = repo.enforce_max_count(1, None).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(images, vec!["evicted.png"]);
        assert!(payloads.is_empty());
        assert!(repo.get_by_id(text).unwrap().is_none());
        assert!(repo.get_by_id(image).unwrap().is_none());
        assert_eq!(
            repo.get_by_id(files)
                .unwrap()
                .unwrap()
                .file_payload
                .as_deref(),
            Some("retained.json")
        );
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
        assert_eq!(
            clip_repo.get_by_id(id).unwrap().unwrap().group_id,
            Some(group.id)
        );

        group_repo.move_item_to_group(id, None).unwrap();
        assert_eq!(clip_repo.get_by_id(id).unwrap().unwrap().group_id, None);
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
