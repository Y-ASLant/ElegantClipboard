use super::{ClipboardItem, ClipboardRepository, ConditionBuilder, QueryOptions};
use rusqlite::{OptionalExtension, Row, params, params_from_iter};

impl ClipboardRepository {
    pub fn get_by_id(&self, id: i64) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        conn.query_row(
            "SELECT * FROM clipboard_items WHERE id = ?1",
            params![id],
            Self::row_to_item,
        )
        .optional()
    }

    /// 按默认排序位置获取完整条目（含文本内容）。
    pub fn get_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        self.query_by_position(index, group_id, false, "*", Self::row_to_item)
    }

    /// 按收藏列表位置获取完整条目。
    pub fn get_favorite_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        self.query_by_position(index, group_id, true, "*", Self::row_to_item)
    }

    /// Resolve a quick-paste slot without loading the item's contents.
    pub fn get_id_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.query_by_position(index, group_id, false, "id", |row| row.get(0))
    }

    pub fn get_favorite_id_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.query_by_position(index, group_id, true, "id", |row| row.get(0))
    }

    fn query_by_position<T>(
        &self,
        index: usize,
        group_id: Option<i64>,
        favorite_only: bool,
        columns: &str,
        map: impl FnOnce(&Row<'_>) -> Result<T, rusqlite::Error>,
    ) -> Result<Option<T>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let builder = ConditionBuilder::filtered(&QueryOptions {
            favorite_only,
            group_id,
            ..Default::default()
        })
        .param(index as i64);
        let order = if favorite_only {
            "is_pinned DESC, favorite_order DESC, sort_order DESC, created_at DESC"
        } else {
            "is_pinned DESC, sort_order DESC, created_at DESC"
        };
        let sql = format!(
            "SELECT {columns} FROM clipboard_items{} ORDER BY {order} LIMIT 1 OFFSET ?",
            builder.where_clause()
        );
        conn.query_row(&sql, params_from_iter(&builder.params), map)
            .optional()
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

    pub fn list(&self, options: QueryOptions) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();

        let is_searching = options.search.as_ref().is_some_and(|s| !s.is_empty());
        let columns = if is_searching {
            Self::SEARCH_COLUMNS
        } else {
            Self::LIST_COLUMNS
        };

        let mut builder = ConditionBuilder::filtered(&options);
        let mut sql = format!(
            "SELECT {columns} FROM clipboard_items{}",
            builder.where_clause()
        );

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
            builder = builder.param(limit).param(options.offset.unwrap_or(0));
        }

        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params_from_iter(&builder.params), Self::row_to_item)?
            .collect()
    }

    pub fn count(&self, options: QueryOptions) -> Result<i64, rusqlite::Error> {
        let conn = self.read_conn.lock();

        ConditionBuilder::filtered(&options).count_items(&conn)
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
}
