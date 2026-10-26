use super::{ClipboardRepository, NewClipboardItem};
use rusqlite::{OptionalExtension, params};
use tracing::debug;

impl ClipboardRepository {
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

        let target_id = match target_id {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // 同文合并别名：默认分组作用域下把别名哈希解析为规范哈希重查一次。
                if group_id.is_some() {
                    return Ok(None);
                }
                let canonical: Option<String> = conn
                    .query_row(
                        "SELECT canonical_hash FROM content_hash_aliases WHERE alias_hash = ?1",
                        params![hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                let Some(canonical) = canonical else {
                    return Ok(None);
                };
                match conn.query_row(
                    "SELECT id FROM clipboard_items \
                     WHERE content_hash = ?1 AND group_id IS NULL \
                     ORDER BY sort_order DESC, created_at DESC, id DESC LIMIT 1",
                    params![canonical],
                    |row| row.get(0),
                ) {
                    Ok(id) => id,
                    Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };

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
            params![new_sort, target_id],
        )?;
        Ok(Some(target_id))
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

    /// 写入条目备注；None 清空。返回 false 表示记录已不存在。
    pub fn set_note(&self, id: i64, note: Option<&str>) -> Result<bool, rusqlite::Error> {
        Ok(self.write_conn.lock().execute(
            "UPDATE clipboard_items SET note = ?1 WHERE id = ?2",
            params![note, id],
        )? > 0)
    }
}
