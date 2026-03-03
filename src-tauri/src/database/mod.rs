mod repository;
mod schema;

pub use repository::*;
pub use schema::*;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
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

        // 迁移 6: 添加 group_id 并将 content_hash 唯一性改为分组内唯一（重建表）
        let has_group_id: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'group_id'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_group_id {
            info!("Migrating database: adding group_id column (table rebuild)");

            // 确认 item_groups 表是否存在（用于迁移旧分组关联数据）
            let has_item_groups: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='item_groups'",
                [],
                |row| row.get(0),
            ).unwrap_or(false);

            let tx = conn.unchecked_transaction()?;

            // 先确保 groups 表存在（schema 顺序已调整，但旧库可能没有）
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );"
            )?;

            // 建新表（含 group_id）
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
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
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );"
            )?;

            // 复制数据：若 item_groups 存在则从中取 MIN(group_id)，否则全部设为 NULL（默认分组）
            if has_item_groups {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon,
                            (SELECT MIN(ig.group_id) FROM item_groups ig WHERE ig.item_id = clipboard_items.id)
                     FROM clipboard_items;"
                )?;
            } else {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon, NULL
                     FROM clipboard_items;"
                )?;
            }

            // 处理重复 hash（同一分组内可能有多条相同 hash 的记录，保留最新一条）
            tx.execute_batch(
                "DELETE FROM clipboard_items_new WHERE id NOT IN (
                    SELECT MAX(id) FROM clipboard_items_new
                    GROUP BY COALESCE(CAST(group_id AS TEXT), 'NULL'), content_hash
                );"
            )?;

            // 删除旧表并重命名
            tx.execute_batch(
                "DROP TABLE clipboard_items;
                 ALTER TABLE clipboard_items_new RENAME TO clipboard_items;
                 DROP TABLE IF EXISTS item_groups;
                 -- 重建索引
                 CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
                 -- 重建触发器
                 CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
                 AFTER UPDATE ON clipboard_items
                 BEGIN
                     UPDATE clipboard_items SET updated_at = datetime('now', 'localtime') WHERE id = new.id;
                 END;"
            )?;

            tx.commit()?;
            info!("Migration complete: group_id column added, table rebuilt");
        }

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
