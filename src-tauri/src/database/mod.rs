mod schema;
mod repository;

pub use schema::*;
pub use repository::*;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// Database manager with read-write separation
/// Uses separate connections for reads and writes to reduce lock contention
/// SQLite WAL mode supports concurrent reads with a single writer
pub struct Database {
    /// Primary connection for write operations (INSERT, UPDATE, DELETE)
    write_conn: Arc<Mutex<Connection>>,
    /// Read-only connection for SELECT queries
    read_conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl Database {
    /// Create a new database with read-write separation
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Create write connection (full access)
        let write_conn = Connection::open(&db_path)?;
        Self::configure_connection(&write_conn, false)?;
        
        // Create read-only connection for queries
        let read_conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_connection(&read_conn, true)?;

        info!("Database opened at {:?} (read-write separation enabled)", db_path);

        let db = Self {
            write_conn: Arc::new(Mutex::new(write_conn)),
            read_conn: Arc::new(Mutex::new(read_conn)),
            db_path,
        };

        // Initialize schema using write connection
        db.init_schema()?;

        Ok(db)
    }

    /// Configure a connection with optimal settings
    fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
        if read_only {
            // Read-only connection optimizations
            conn.execute_batch(
                "PRAGMA query_only = ON;
                 PRAGMA cache_size = -32000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;"
            )?;
        } else {
            // Write connection with WAL mode
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA foreign_keys = ON;"
            )?;
        }
        Ok(())
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        
        // Run migrations FIRST (for existing databases)
        Self::run_migrations(&conn)?;
        
        // Then execute schema (CREATE IF NOT EXISTS is safe for new tables/indexes)
        conn.execute_batch(SCHEMA_SQL)?;
        info!("Database schema initialized");
        
        Ok(())
    }
    
    /// Run database migrations for schema updates
    /// This runs BEFORE schema creation to handle existing databases
    fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        // Check if clipboard_items table exists (skip migrations for new databases)
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !table_exists {
            // New database, no migrations needed
            return Ok(());
        }

        // Migration 1: Add sort_order column
        let has_sort_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_sort_order {
            info!("Migrating database: adding sort_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN sort_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items SET sort_order = id;"
            )?;
            info!("Migration complete: sort_order column added");
        }

        // Migration 2: Drop FTS5 table and triggers (replaced by LIKE search for CJK support)
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
                 DROP TABLE IF EXISTS clipboard_fts;"
            )?;
            info!("Migration complete: FTS5 removed");
        }

        // Migration 3: Add image_width and image_height columns
        let has_image_width: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'image_width'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_image_width {
            info!("Migrating database: adding image_width and image_height columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN image_width INTEGER;
                 ALTER TABLE clipboard_items ADD COLUMN image_height INTEGER;"
            )?;
            info!("Migration complete: image_width and image_height columns added");
        }

        Ok(())
    }

    /// Get a reference to the write connection (for INSERT, UPDATE, DELETE)
    pub fn write_connection(&self) -> Arc<Mutex<Connection>> {
        self.write_conn.clone()
    }

    /// Get a reference to the read connection (for SELECT queries)
    pub fn read_connection(&self) -> Arc<Mutex<Connection>> {
        self.read_conn.clone()
    }

    /// Get database path
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Optimize database (call periodically)
    pub fn optimize(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute_batch("PRAGMA optimize;")?;
        info!("Database optimized");
        Ok(())
    }

    /// Vacuum database to reclaim space
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

/// Get the default database path
pub fn get_default_db_path() -> PathBuf {
    let app_data = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    app_data.join("ElegantClipboard").join("clipboard.db")
}

/// Get the default images storage path
pub fn get_default_images_path() -> PathBuf {
    let app_data = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    app_data.join("ElegantClipboard").join("images")
}
