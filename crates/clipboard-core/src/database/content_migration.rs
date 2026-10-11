use super::Database;
use crate::clipboard::is_url;
use rusqlite::{Connection, params};
use tracing::info;

impl Database {
    fn url_content_type_migration_done(conn: &Connection) -> Result<bool, rusqlite::Error> {
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !settings_exist {
            return Ok(false);
        }
        conn.query_row(
            "SELECT COALESCE((SELECT value FROM settings WHERE key = '_migration_url_content_type' LIMIT 1), '') = 'done'",
            [],
            |row| row.get(0),
        )
    }

    pub(super) fn mark_url_content_type_migration_done(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        let settings_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='settings'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if settings_exist {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('_migration_url_content_type', 'done')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value WHERE value != excluded.value",
                [],
            )?;
        }
        Ok(())
    }

    pub(super) fn migrate_url_content_type(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 旧库：迁移 10 曾通过重建表把 'url' 写进 CHECK 约束
        let legacy_done: bool = conn
            .query_row(
                "SELECT COALESCE((SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'), '') LIKE '%''url''%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if legacy_done || Self::url_content_type_migration_done(conn)? {
            Self::mark_url_content_type_migration_done(conn)?;
            return Ok(());
        }

        info!("Migrating database: adding url content_type");
        conn.execute_batch(
            "UPDATE clipboard_items
             SET semantic_hash = content_hash
             WHERE semantic_hash IS NULL OR semantic_hash = '';",
        )?;

        Self::strip_content_type_check_if_present(conn)?;
        Self::backfill_url_content_types(conn)?;
        Self::mark_url_content_type_migration_done(conn)?;
        info!("Migration complete: url content_type added");
        Ok(())
    }

    /// 旧表若仍带 content_type CHECK，回填 url 前先去约束（重建表）
    fn strip_content_type_check_if_present(conn: &Connection) -> Result<(), rusqlite::Error> {
        let has_check: bool = conn
            .query_row(
                "SELECT COALESCE((SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'), '') LIKE '%CHECK(content_type%'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_check {
            return Ok(());
        }

        let has_file_payload: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'file_payload'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        info!("Migrating database: removing content_type CHECK constraint");
        let tx = conn.unchecked_transaction()?;
        if has_file_payload {
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    file_payload TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                INSERT INTO clipboard_items_new
                SELECT id, content_type, text_content, html_content, rtf_content,
                       image_path, file_paths, file_payload, content_hash,
                       COALESCE(NULLIF(semantic_hash, ''), content_hash),
                       preview, byte_size, image_width, image_height,
                       is_pinned, is_favorite, favorite_order, sort_order,
                       created_at, updated_at, access_count, last_accessed_at,
                       char_count, source_app_name, source_app_icon, group_id
                FROM clipboard_items;
                DROP TABLE clipboard_items;
                ALTER TABLE clipboard_items_new RENAME TO clipboard_items;",
            )?;
        } else {
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                INSERT INTO clipboard_items_new
                SELECT id, content_type, text_content, html_content, rtf_content,
                       image_path, file_paths, content_hash,
                       COALESCE(NULLIF(semantic_hash, ''), content_hash),
                       preview, byte_size, image_width, image_height,
                       is_pinned, is_favorite, favorite_order, sort_order,
                       created_at, updated_at, access_count, last_accessed_at,
                       char_count, source_app_name, source_app_icon, group_id
                FROM clipboard_items;
                DROP TABLE clipboard_items;
                ALTER TABLE clipboard_items_new RENAME TO clipboard_items;",
            )?;
        }
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
             CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
             AFTER UPDATE ON clipboard_items
             BEGIN
                 UPDATE clipboard_items SET updated_at = datetime('now', 'localtime')
                 WHERE id = new.id;
             END;",
        )?;
        tx.commit()?;
        Ok(())
    }

    fn backfill_url_content_types(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, text_content FROM clipboard_items WHERE content_type = 'text' AND text_content IS NOT NULL",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();

        let mut update =
            conn.prepare_cached("UPDATE clipboard_items SET content_type = 'url', semantic_hash = content_hash WHERE id = ?1")?;
        let mut count = 0;
        for (id, text) in rows {
            if is_url(&text) {
                update.execute(params![id])?;
                count += 1;
            }
        }
        if count > 0 {
            info!("Backfilled {} url content types", count);
        }
        Ok(())
    }
}
