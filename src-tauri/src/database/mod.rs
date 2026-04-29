mod repository;
mod schema;

pub use repository::*;
pub use schema::*;

use crate::clipboard::compute_semantic_hash;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// 数据库管理器（读写分离）
pub struct Database {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let write_conn = Connection::open(&db_path)?;
        Self::configure_connection(&write_conn, false)?;

        let read_conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_connection(&read_conn, true)?;

        info!("Database opened at {:?}", db_path);

        let db = Self {
            write_conn: Arc::new(Mutex::new(write_conn)),
            read_conn: Arc::new(Mutex::new(read_conn)),
            db_path,
        };

        db.init_schema()?;

        Ok(db)
    }

    fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
        if read_only {
            conn.execute_batch(
                "PRAGMA query_only = ON;
                 PRAGMA cache_size = -32000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;",
            )?;
        } else {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA foreign_keys = ON;",
            )?;
        }
        Ok(())
    }

    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();

        Self::run_migrations(&conn)?;

        conn.execute_batch(SCHEMA_SQL)?;
        info!("Database schema initialized");

        Ok(())
    }

    /// 数据库迁移（在 schema 创建前执行）
    fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !table_exists {
            return Ok(());
        }

        // 迁移 1: sort_order
        let has_sort_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_sort_order {
            info!("Migrating database: adding sort_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN sort_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items SET sort_order = id;",
            )?;
            info!("Migration complete: sort_order column added");
        }

        // 迁移 2: 移除 FTS5（改用 LIKE 支持中文搜索）
        let has_fts: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_fts'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if has_fts {
            info!("Migrating database: removing FTS5 table and triggers");
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS clipboard_items_ai;
                 DROP TRIGGER IF EXISTS clipboard_items_ad;
                 DROP TRIGGER IF EXISTS clipboard_items_au;
                 DROP TABLE IF EXISTS clipboard_fts;",
            )?;
            info!("Migration complete: FTS5 removed");
        }

        // 迁移 3: char_count
        let has_char_count: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'char_count'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_char_count {
            info!("Migrating database: adding char_count column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN char_count INTEGER;
                 UPDATE clipboard_items SET char_count = LENGTH(text_content) WHERE text_content IS NOT NULL;"
            )?;
            info!("Migration complete: char_count column added");
        }

        // 迁移 4: image_width/image_height
        let has_image_width: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'image_width'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_image_width {
            info!("Migrating database: adding image_width and image_height columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN image_width INTEGER;
                 ALTER TABLE clipboard_items ADD COLUMN image_height INTEGER;",
            )?;
            info!("Migration complete: image_width and image_height columns added");
        }

        // 迁移 5: source_app_name/source_app_icon
        let has_source_app: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'source_app_name'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_source_app {
            info!("Migrating database: adding source_app_name and source_app_icon columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN source_app_name TEXT;
                 ALTER TABLE clipboard_items ADD COLUMN source_app_icon TEXT;",
            )?;
            info!("Migration complete: source_app columns added");
        }

        // 迁移 6: 重建表（添加 semantic_hash 列，清理旧 group 相关结构）
        let has_semantic_hash_m6: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'semantic_hash'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_semantic_hash_m6 {
            info!("Migrating database: rebuilding table (add semantic_hash, clean up legacy columns)");

            let tx = conn.unchecked_transaction()?;

            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'video')),
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
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT
                );"
            )?;

            tx.execute_batch(
                "INSERT INTO clipboard_items_new
                 SELECT id, content_type, text_content, html_content, rtf_content,
                        image_path, file_paths, content_hash, content_hash, preview, byte_size,
                        image_width, image_height, is_pinned, is_favorite, sort_order,
                        created_at, updated_at, access_count, last_accessed_at, char_count,
                        source_app_name, source_app_icon
                 FROM clipboard_items;"
            )?;

            tx.execute_batch(
                "DROP TABLE clipboard_items;
                 ALTER TABLE clipboard_items_new RENAME TO clipboard_items;
                 DROP TABLE IF EXISTS item_groups;
                 DROP TABLE IF EXISTS groups;
                 -- 重建索引
                 CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash ON clipboard_items(semantic_hash);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
                 -- 重建触发器
                 CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
                 AFTER UPDATE ON clipboard_items
                 BEGIN
                     UPDATE clipboard_items SET updated_at = datetime('now', 'localtime') WHERE id = new.id;
                 END;"
            )?;

            tx.commit()?;
            info!("Migration complete: table rebuilt with semantic_hash");
        }

        // 迁移 7: 清理遗留的 group 相关索引和列
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_clipboard_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_hash_group;
             DROP INDEX IF EXISTS idx_clipboard_semantic_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_semantic_hash_group;
             DROP INDEX IF EXISTS idx_clipboard_group;
             DROP TABLE IF EXISTS groups;
             DROP TABLE IF EXISTS item_groups;",
        )?;

        // Migration 8: add semantic_hash and backfill existing rows.
        let has_semantic_hash: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'semantic_hash'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_semantic_hash {
            info!("Migrating database: adding semantic_hash column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN semantic_hash TEXT;",
            )?;
        }

        Self::backfill_semantic_hashes(conn)?;

        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_clipboard_semantic_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_semantic_hash_group;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash
               ON clipboard_items(semantic_hash);",
        )?;

        // Migration 9: replace groups with tags system
        let has_tags_table: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='tags'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_tags_table {
            info!("Migrating database: creating tags system");
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );
                CREATE TABLE IF NOT EXISTS item_tags (
                    item_id INTEGER NOT NULL REFERENCES clipboard_items(id) ON DELETE CASCADE,
                    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    PRIMARY KEY (item_id, tag_id)
                );
                CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);",
            )?;
            info!("Migration complete: tags system created");
        }

        // Migration 10: add sort_order column to tags table
        let has_sort_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('tags') WHERE name='sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_sort_order {
            info!("Migrating database: adding sort_order to tags");
            conn.execute_batch(
                "ALTER TABLE tags ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
                 UPDATE tags SET sort_order = id;",
            )?;
            info!("Migration complete: tags sort_order added");
        }

        // Migration 11: add sort_order to item_tags for custom item ordering within a tag
        let has_item_tags_sort: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('item_tags') WHERE name='sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_item_tags_sort {
            info!("Migrating database: adding sort_order to item_tags");
            conn.execute_batch(
                "ALTER TABLE item_tags ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;",
            )?;
            info!("Migration complete: item_tags sort_order added");
        }

        // Migration 12: add files_valid column for persistent file validity tracking
        let has_files_valid: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name='files_valid'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_files_valid {
            info!("Migrating database: adding files_valid column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN files_valid INTEGER DEFAULT 1;",
            )?;
            info!("Migration complete: files_valid column added");
        }

        // Migration 13: add 'video' to content_type CHECK constraint
        // Check if the CHECK constraint already includes 'video' by trying an insert
        let needs_video_migration = {
            let test_result = conn.execute(
                "INSERT INTO clipboard_items (content_type, content_hash, semantic_hash) VALUES ('video', '__test__', '__test__')",
                [],
            );
            match test_result {
                Ok(_) => {
                    // Clean up test row
                    conn.execute("DELETE FROM clipboard_items WHERE content_hash = '__test__'", []).ok();
                    false // constraint already allows 'video'
                }
                Err(_) => true, // constraint rejects 'video', need migration
            }
        };

        if needs_video_migration {
            info!("Migrating database: adding 'video' to content_type CHECK constraint");
            let tx = conn.unchecked_transaction()?;

            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'video')),
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
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    files_valid INTEGER DEFAULT 1
                );
                INSERT INTO clipboard_items_new (id, content_type, text_content, html_content, rtf_content, image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, source_app_name, source_app_icon, files_valid)
                SELECT id, content_type, text_content, html_content, rtf_content, image_path, file_paths, content_hash, semantic_hash, preview, byte_size, image_width, image_height, is_pinned, is_favorite, sort_order, created_at, updated_at, access_count, last_accessed_at, char_count, source_app_name, source_app_icon, files_valid FROM clipboard_items;
                DROP TABLE clipboard_items;
                ALTER TABLE clipboard_items_new RENAME TO clipboard_items;"
            )?;

            // Migrate existing video files from 'files' to 'video' content_type
            Self::migrate_video_content_type(&tx);

            tx.commit()?;
            info!("Migration complete: 'video' content_type added");
        } else {
            // Even if constraint already allows 'video', migrate existing video files
            Self::migrate_video_content_type(conn);
        }

        // Drop old group-related indexes (safe even if they don't exist)
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_clipboard_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_hash_group;
             DROP INDEX IF EXISTS idx_clipboard_group;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);",
        )?;

        Ok(())
    }

    /// Migrate existing 'files' type items that are actually videos to 'video' content_type
    fn migrate_video_content_type(conn: &Connection) {
        const VIDEO_EXTENSIONS: &[&str] = &[
            "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpeg", "mpg",
        ];

        let mut stmt = match conn.prepare(
            "SELECT id, file_paths FROM clipboard_items WHERE content_type = 'files' AND file_paths IS NOT NULL"
        ) {
            Ok(s) => s,
            Err(_) => return,
        };

        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        drop(stmt);

        let mut count = 0usize;
        for (id, paths_json) in &rows {
            let paths: Vec<String> = serde_json::from_str(paths_json).unwrap_or_default();
            if !paths.is_empty() && paths.iter().all(|f| {
                let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                VIDEO_EXTENSIONS.contains(&ext.as_str())
            }) {
                if conn.execute(
                    "UPDATE clipboard_items SET content_type = 'video' WHERE id = ?1",
                    rusqlite::params![id],
                ).is_ok() {
                    count += 1;
                }
            }
        }
        if count > 0 {
            info!("Migrated {} files items to video content_type", count);
        }
    }

    fn backfill_semantic_hashes(conn: &Connection) -> Result<(), rusqlite::Error> {
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

        if updates.is_empty() {
            return Ok(());
        }

        let updated_count = updates.len();
        let tx = conn.unchecked_transaction()?;
        {
            let mut update_stmt =
                tx.prepare("UPDATE clipboard_items SET semantic_hash = ?1 WHERE id = ?2")?;
            for (id, semantic_hash) in updates {
                update_stmt.execute(params![semantic_hash, id])?;
            }
        }
        tx.commit()?;
        info!(
            "Migration complete: semantic_hash backfilled for {} rows",
            updated_count
        );
        Ok(())
    }

    pub fn write_connection(&self) -> Arc<Mutex<Connection>> {
        self.write_conn.clone()
    }

    pub fn read_connection(&self) -> Arc<Mutex<Connection>> {
        self.read_conn.clone()
    }

    pub fn optimize(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute_batch("PRAGMA optimize;")?;
        info!("Database optimized");
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute_batch("VACUUM;")?;
        info!("Database vacuumed");
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            write_conn: self.write_conn.clone(),
            read_conn: self.read_conn.clone(),
            db_path: self.db_path.clone(),
        }
    }
}

/// 获取应用安装目录（可执行文件所在目录）
pub fn get_app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_default_db_path() -> PathBuf {
    get_app_dir().join("clipboard.db")
}

pub fn get_default_images_path() -> PathBuf {
    get_app_dir().join("images")
}
