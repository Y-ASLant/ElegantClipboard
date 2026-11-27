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

#[cfg(test)]
pub(crate) const V1_2_10_SCHEMA: &str = include_str!("v1_2_10.sql");

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
        let mut conn = self.write_conn.lock();

        // Reserve the writer before inspecting the schema: a concurrent capture
        // can otherwise invalidate the WAL snapshot before the schema writes.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let upgraded_from_1_2_10 = Self::run_migrations(&tx)?;
        tx.execute_batch(SCHEMA_SQL)?;
        if upgraded_from_1_2_10 {
            crate::legacy_migration::mark_cleanup_ui_needed(&tx)?;
        }
        tx.commit()?;
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
        Self::rebuild_search_index_once(conn)?;
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

    /// 读写双连接时直接 VACUUM 会拿不到 exclusive lock（Windows 上常见“整理了体积不变”）。
    /// 先关掉只读连接，checkpoint + VACUUM，再挂回只读连接。
    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let write = self.write_conn.lock();
        let mut read = self.read_conn.lock();
        // 释放只读句柄，否则 VACUUM 无法独占文件。
        *read = Connection::open_in_memory()?;
        write.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE);
             VACUUM;
             PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
        let reopened = Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_connection(&reopened, true)?;
        *read = reopened;
        info!("Database vacuumed");
        Ok(())
    }
}
