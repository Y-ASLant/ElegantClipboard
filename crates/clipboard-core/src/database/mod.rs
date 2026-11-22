mod content_migration;
mod data_migration;
mod groups;
mod migrations;
mod settings;

pub use groups::GroupRepository;
pub use settings::SettingsRepository;

mod repository;
mod schema;

pub use repository::*;
pub use schema::*;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// 数据库管理器（读写分离）
#[derive(Clone)]
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

    /// 数据库文件路径；其父目录即数据目录。
    pub fn path(&self) -> &std::path::Path {
        &self.db_path
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
        Self::run_post_schema_migrations(&conn)?;
        info!("Database schema initialized");

        Ok(())
    }

    /// URL 类型、语义哈希与收藏顺序等数据迁移；在 `SCHEMA_SQL` 之后运行以便 `settings` 等表已存在。
    fn run_post_schema_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::migrate_url_content_type(conn)?;
        Self::backfill_categories(conn)?;
        Self::backfill_semantic_hashes(conn)?;
        Self::normalize_favorite_order(conn)?;
        // 旧库可能仍是「任意列 UPDATE 都改 FTS」；与 timestamp 触发器叠在一起会 SQLITE_CORRUPT_INDEX。
        Self::ensure_fts_update_trigger(conn)?;
        Self::rebuild_search_index_once(conn)?;
        Ok(())
    }

    /// FTS 只跟踪正文/路径/备注；`CREATE TRIGGER IF NOT EXISTS` 不会替换旧定义。
    fn ensure_fts_update_trigger(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS clipboard_fts_au;
             CREATE TRIGGER clipboard_fts_au
             AFTER UPDATE OF text_content, file_paths, note ON clipboard_items
             BEGIN
                 INSERT INTO clipboard_fts(clipboard_fts, rowid, text_content, file_paths, note)
                 VALUES ('delete', old.id, old.text_content, old.file_paths, old.note);
                 INSERT INTO clipboard_fts(rowid, text_content, file_paths, note)
                 VALUES (new.id, new.text_content, new.file_paths, new.note);
             END;",
        )?;
        Ok(())
    }

    /// One-time FTS backfill. Later writes stay in sync through schema triggers.
    fn rebuild_search_index_once(conn: &Connection) -> Result<(), rusqlite::Error> {
        let done: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = '_migration_clipboard_fts'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if done.is_some() {
            return Ok(());
        }
        conn.execute_batch("INSERT INTO clipboard_fts(clipboard_fts) VALUES('rebuild');")?;
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('_migration_clipboard_fts', '1', datetime('now', 'localtime'))",
            [],
        )?;
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
