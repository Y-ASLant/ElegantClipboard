use super::Database;
use crate::clipboard::compute_semantic_hash;
use rusqlite::{Connection, params};
use tracing::info;

impl Database {
    /// 将所有 `is_favorite = 1` 的条目的 `favorite_order` 规整为
    /// 单调递增、互不相同的整数，保留当前显示顺序。
    /// 当不存在重复或零值时直接跳过，避免无谓写入。
    pub(super) fn normalize_favorite_order(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 是否存在并列或为零的 favorite_order
        let needs_fix: bool = conn.query_row(
            "SELECT (COUNT(*) > COUNT(DISTINCT favorite_order)) \
             OR EXISTS(SELECT 1 FROM clipboard_items WHERE is_favorite = 1 AND favorite_order <= 0) \
             FROM clipboard_items WHERE is_favorite = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !needs_fix {
            return Ok(());
        }

        // 取出所有收藏项 id，按当前显示顺序排列（最前面的在最前）
        let ids: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM clipboard_items \
                 WHERE is_favorite = 1 \
                 ORDER BY favorite_order DESC, id DESC",
            )?;
            stmt.query_map([], |row| row.get::<_, i64>(0))?
                .filter_map(std::result::Result::ok)
                .collect()
        };

        if ids.is_empty() {
            return Ok(());
        }

        info!(
            "Migrating database: normalizing favorite_order for {} items",
            ids.len()
        );

        let total = ids.len() as i64;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE clipboard_items SET favorite_order = ?1 \
                 WHERE id = ?2 AND is_favorite = 1",
            )?;
            for (idx, id) in ids.iter().enumerate() {
                let new_order = total - idx as i64; // 最前面的得到最大值
                stmt.execute(params![new_order, id])?;
            }
        }
        tx.commit()?;

        info!("Migration complete: favorite_order normalized");
        Ok(())
    }

    pub(super) fn backfill_semantic_hashes(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 版本守卫：已完成的迁移不再执行，避免每次启动全表扫描
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if settings_exist {
            let already_done: bool = conn
                .query_row(
                    "SELECT COALESCE((SELECT value FROM settings WHERE key = '_migration_backfill_semantic_hash' LIMIT 1), '') = 'done'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if already_done {
                return Ok(());
            }
        }

        let mut stmt = conn.prepare(
            "SELECT id, content_type, text_content, content_hash, semantic_hash
             FROM clipboard_items
             WHERE semantic_hash IS NULL
                OR semantic_hash = ''
                OR (content_type IN ('text', 'html', 'rtf') AND semantic_hash = content_hash)",
        )?;

        let mut updates: Vec<(i64, String)> = Vec::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        for row in rows {
            let (id, content_type, text_content, content_hash, existing_semantic_hash) = row?;
            let computed_semantic_hash =
                compute_semantic_hash(&content_type, text_content.as_deref(), &content_hash);

            if existing_semantic_hash.as_deref() != Some(computed_semantic_hash.as_str()) {
                updates.push((id, computed_semantic_hash));
            }
        }
        drop(stmt);

        let tx = conn.unchecked_transaction()?;
        if !updates.is_empty() {
            let updated_count = updates.len();
            {
                let mut update_stmt =
                    tx.prepare("UPDATE clipboard_items SET semantic_hash = ?1 WHERE id = ?2")?;
                for (id, semantic_hash) in updates {
                    update_stmt.execute(params![semantic_hash, id])?;
                }
            }
            info!(
                "Migration complete: semantic_hash backfilled for {} rows",
                updated_count
            );
        }

        // 写入版本标记，后续启动跳过
        if settings_exist {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('_migration_backfill_semantic_hash', 'done')",
                [],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
