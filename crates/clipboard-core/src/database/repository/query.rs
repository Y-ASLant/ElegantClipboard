use super::{ClipboardItem, ClipboardRepository, ConditionBuilder, QueryOptions};
use rusqlite::{OptionalExtension, Row, params, params_from_iter};

impl ClipboardRepository {
    pub fn get_by_id(&self, id: i64) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut item = conn
            .query_row(
                "SELECT * FROM clipboard_items WHERE id = ?1",
                params![id],
                Self::row_to_item,
            )
            .optional()?;
        if let Some(item) = item.as_mut() {
            fill_formats(&conn, std::slice::from_mut(item))?;
        }
        Ok(item)
    }

    /// Resolve a quick-paste slot without loading the item's contents.
    pub fn get_id_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.query_id_by_position(index, group_id, false)
    }

    pub fn get_favorite_id_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, rusqlite::Error> {
        self.query_id_by_position(index, group_id, true)
    }

    fn query_id_by_position(
        &self,
        index: usize,
        group_id: Option<i64>,
        favorite_only: bool,
    ) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let builder = ConditionBuilder::filtered(&QueryOptions {
            favorite_only,
            group_id,
            ..Default::default()
        })
        .param(index as i64);
        let sql = format!(
            "SELECT id FROM clipboard_items{}{}{} LIMIT 1 OFFSET ?",
            Self::order_index(&QueryOptions {
                favorite_only,
                group_id,
                ..Default::default()
            }),
            builder.where_clause(),
            Self::order_clause(favorite_only)
        );
        conn.query_row(&sql, params_from_iter(&builder.params), |row| row.get(0))
            .optional()
    }

    /// The planner prefers `idx_clipboard_group` and then sorts. These indexes already
    /// match the list order, so non-search reads name them explicitly.
    fn order_index(options: &QueryOptions) -> &'static str {
        if options
            .search
            .as_ref()
            .is_some_and(|search| !search.is_empty())
        {
            return "";
        }
        match (options.group_id, options.favorite_only) {
            (None, false) => " INDEXED BY idx_clipboard_default_list",
            (None, true) => " INDEXED BY idx_clipboard_favorite_default_list",
            (Some(_), false) => " INDEXED BY idx_clipboard_group_list",
            (Some(_), true) => " INDEXED BY idx_clipboard_favorite_group_list",
        }
    }

    fn order_clause(favorite_only: bool) -> &'static str {
        if favorite_only {
            " ORDER BY is_favorite_pinned DESC, favorite_order DESC, sort_order DESC, created_at DESC"
        } else {
            " ORDER BY is_pinned DESC, sort_order DESC, created_at DESC"
        }
    }

    /// 列表查询列（排除大文本字段以减少 IPC 传输）
    const LIST_COLUMNS: &'static str = "id, content_type, NULL AS text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, NULL AS file_payload, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, note, group_id";

    /// 搜索查询列（含 text_content 用于关键词上下文预览）
    const SEARCH_COLUMNS: &'static str = "id, content_type, text_content, NULL AS html_content, NULL AS rtf_content, \
         image_path, file_paths, NULL AS file_payload, content_hash, semantic_hash, preview, byte_size, image_width, image_height, \
         is_pinned, is_favorite_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, \
         source_app_name, source_app_icon, note, group_id";

    pub fn list(&self, options: QueryOptions) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
        let conn = self.read_conn.lock();

        let is_searching = options.search.as_ref().is_some_and(|s| !s.is_empty());
        let columns = if is_searching && !options.omit_text {
            Self::SEARCH_COLUMNS
        } else {
            Self::LIST_COLUMNS
        };

        let mut builder = ConditionBuilder::filtered(&options);
        let mut sql = format!(
            "SELECT {columns} FROM clipboard_items{}{}",
            Self::order_index(&options),
            builder.where_clause()
        );

        sql.push_str(Self::order_clause(options.favorite_only));

        if let Some(limit) = options.limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            builder = builder.param(limit).param(options.offset.unwrap_or(0));
        }

        let mut stmt = conn.prepare(&sql)?;
        let mut items = stmt
            .query_map(params_from_iter(&builder.params), Self::row_to_item)?
            .collect::<Result<Vec<_>, _>>()?;
        fill_formats(&conn, &mut items)?;
        if options.omit_text
            && let Some(search) = options.search.as_deref().filter(|s| !s.is_empty())
        {
            fill_search_excerpts(&conn, &mut items, search)?;
        }
        Ok(items)
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
            is_favorite_pinned: row.get("is_favorite_pinned")?,
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
            note: row.get("note")?,
            group_id: row.get("group_id")?,
            formats: Vec::new(),
        })
    }
}

/// 按记录 ID 批量回填实际采集的格式名；id 均为整数，直接内联到 IN 列表。
fn fill_formats(
    conn: &rusqlite::Connection,
    items: &mut [ClipboardItem],
) -> Result<(), rusqlite::Error> {
    if items.is_empty() {
        return Ok(());
    }
    let ids = items
        .iter()
        .map(|item| item.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = conn.prepare(&format!(
        "SELECT item_id, format_name FROM captured_formats \
         WHERE item_id IN ({ids}) ORDER BY format_name"
    ))?;
    let mut rows = statement.query([])?;
    let mut names = std::collections::HashMap::<i64, Vec<String>>::new();
    while let Some(row) = rows.next()? {
        names
            .entry(row.get::<_, i64>(0)?)
            .or_default()
            .push(row.get::<_, String>(1)?);
    }
    for item in items {
        if let Some(item_names) = names.remove(&item.id) {
            item.formats = item_names;
        }
    }
    Ok(())
}

/// Load bodies only for rows whose preview does not already show the term.
fn fill_search_excerpts(
    conn: &rusqlite::Connection,
    items: &mut [ClipboardItem],
    search: &str,
) -> Result<(), rusqlite::Error> {
    // File searches can match a directory or an unlisted filename, not the
    // displayed basename. Use the decoded path, never its JSON serialization.
    for item in items
        .iter_mut()
        .filter(|item| item.content_type == "files" && item.note.is_some())
    {
        let preview = item.preview.as_deref().unwrap_or_default();
        if crate::search::search_excerpt_needed(preview, search)
            && crate::search::first_search_match(item.note.as_deref().unwrap_or_default(), search)
                .is_none()
            && let Some(excerpt) = crate::parse_file_paths_for_display(item.file_paths.as_deref())
                .iter()
                .find_map(|path| crate::search::search_excerpt(path, "", search))
        {
            item.preview = Some(excerpt);
        }
    }
    let needed: Vec<i64> = items
        .iter()
        .filter(|item| {
            item.content_type != "image"
                && crate::search::search_excerpt_needed(
                    item.preview.as_deref().unwrap_or(""),
                    search,
                )
        })
        .map(|item| item.id)
        .collect();
    if needed.is_empty() {
        return Ok(());
    }
    let ids = needed
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = conn.prepare(&format!(
        "SELECT id, text_content FROM clipboard_items WHERE id IN ({ids})"
    ))?;
    let mut texts = std::collections::HashMap::<i64, String>::new();
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if let Some(text) = row.get::<_, Option<String>>(1)? {
            texts.insert(row.get(0)?, text);
        }
    }
    for item in items.iter_mut() {
        let Some(full) = texts.remove(&item.id) else {
            continue;
        };
        if let Some(excerpt) =
            crate::search::search_excerpt(&full, item.preview.as_deref().unwrap_or(""), search)
        {
            item.preview = Some(excerpt);
        }
    }
    Ok(())
}
