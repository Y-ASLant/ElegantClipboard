mod schema;
mod repository;

pub use schema::*;
pub use repository::*;

use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// Database manager with connection pooling
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl Database {
    /// Create a new database connection
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(&db_path)?;
        
        // Configure for high performance
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 268435456;
             PRAGMA foreign_keys = ON;"
        )?;

        info!("Database opened at {:?}", db_path);

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        };

        // Initialize schema
        db.init_schema()?;

        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        
        conn.execute_batch(SCHEMA_SQL)?;
        
        info!("Database schema initialized");
        Ok(())
    }

    /// Get a reference to the connection
    pub fn connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Get database path
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Optimize database (call periodically)
    pub fn optimize(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute_batch("PRAGMA optimize;")?;
        info!("Database optimized");
        Ok(())
    }

    /// Vacuum database to reclaim space
    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute_batch("VACUUM;")?;
        info!("Database vacuumed");
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
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
