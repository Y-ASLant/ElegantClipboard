use super::{ContentType, Database};
use parking_lot::Mutex;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// Clipboard item model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: i64,
    pub content_type: String,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<String>,
    pub content_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub is_pinned: bool,
    pub is_favorite: bool,
    pub category_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: i64,
    pub last_accessed_at: Option<String>,
}

/// Category model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub icon: String,
    pub sort_order: i64,
    pub created_at: String,
}

/// New clipboard item (for insertion)
#[derive(Debug, Clone)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub content_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
}

/// Query options for listing items
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOptions {
    pub search: Option<String>,
    pub content_type: Option<String>,
    pub category_id: Option<i64>,
    pub pinned_only: bool,
    pub favorite_only: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Repository for clipboard items
pub struct ClipboardRepository {
    conn: Arc<Mutex<Connection>>,
}

impl ClipboardRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Insert a new clipboard item
    pub fn insert(&self, item: NewClipboardItem) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock();
        
        let file_paths_json = item.file_paths.map(|paths| serde_json::to_string(&paths).unwrap_or_default());
        
        conn.execute(
            "INSERT INTO clipboard_items (content_type, text_content, html_content, rtf_content, image_path, file_paths, content_hash, preview, byte_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                item.content_type.as_str(),
                item.text_content,
                item.html_content,
                item.rtf_content,
                item.image_path,
                file_paths_json,
                item.content_hash,
                item.preview,
                item.byte_size,
            ],
        )?;

        let id = conn.last_insert_rowid();
        debug!("Inserted clipboard item with id: {}", id);
        Ok(id)
    }

    /// Check if item with hash exists
    pub fn exists_by_hash(&self, hash: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get item by hash and update access time
    pub fn touch_by_hash(&self, hash: &str) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.conn.lock();
        
        // Update access count and time
        conn.execute(
            "UPDATE clipboard_items 
             SET access_count = access_count + 1, 
                 last_accessed_at = datetime('now', 'localtime'),
                 updated_at = datetime('now', 'localtime')
             WHERE content_hash = ?1",
            params![hash],
        )?;

        // Get the id
        let result: Result<i64, _> = conn.query_row(
            "SELECT id FROM clipboard_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        );

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get item by ID
    pub fn get_by_id(&self, id: i64) -> Result<Option<ClipboardItem>, rusqlite::Error> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT * FROM clipboard_items WHERE id = ?1",
            params![id],
            Self::row_to_item,
        );

        match result {
            Ok(item) => Ok(Some(item)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List items with query options
    pub fn list(&self, options: QueryOptions) -> Result<Vec<ClipboardItem>, rusqlite::Error> {
        let conn = self.conn.lock();
        
        let mut sql = String::from(
            "SELECT clipboard_items.* FROM clipboard_items"
        );
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Full-text search
        if let Some(ref search) = options.search {
            if !search.is_empty() {
                sql = format!(
                    "SELECT clipboard_items.* FROM clipboard_items 
                     INNER JOIN clipboard_fts ON clipboard_items.id = clipboard_fts.rowid"
                );
                conditions.push("clipboard_fts MATCH ?".to_string());
                params_vec.push(Box::new(format!("{}*", search)));
            }
        }

        // Filter by content type
        if let Some(ref content_type) = options.content_type {
            conditions.push("content_type = ?".to_string());
            params_vec.push(Box::new(content_type.clone()));
        }

        // Filter by category
        if let Some(category_id) = options.category_id {
            conditions.push("category_id = ?".to_string());
            params_vec.push(Box::new(category_id));
        }

        // Filter pinned only
        if options.pinned_only {
            conditions.push("is_pinned = 1".to_string());
        }

        // Filter favorite only
        if options.favorite_only {
            conditions.push("is_favorite = 1".to_string());
        }

        // Build WHERE clause
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        // Order by: pinned first, then by created_at
        sql.push_str(" ORDER BY is_pinned DESC, created_at DESC");

        // Limit and offset
        let limit = options.limit.unwrap_or(100);
        let offset = options.offset.unwrap_or(0);
        sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        // Execute query
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let items = stmt
            .query_map(params_refs.as_slice(), Self::row_to_item)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Get total count
    pub fn count(&self, options: QueryOptions) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock();
        
        let mut sql = String::from("SELECT COUNT(*) FROM clipboard_items");
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref content_type) = options.content_type {
            conditions.push("content_type = ?".to_string());
            params_vec.push(Box::new(content_type.clone()));
        }

        if options.pinned_only {
            conditions.push("is_pinned = 1".to_string());
        }

        if options.favorite_only {
            conditions.push("is_favorite = 1".to_string());
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count)
    }

    /// Toggle pin status
    pub fn toggle_pin(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock();
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

    /// Toggle favorite status
    pub fn toggle_favorite(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET is_favorite = NOT is_favorite WHERE id = ?1",
            params![id],
        )?;
        
        let favorite: bool = conn.query_row(
            "SELECT is_favorite FROM clipboard_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        
        Ok(favorite)
    }

    /// Delete item by ID
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        debug!("Deleted clipboard item with id: {}", id);
        Ok(())
    }

    /// Delete all non-pinned items
    pub fn clear_history(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
        )?;
        Ok(deleted as i64)
    }

    /// Delete items older than days
    #[allow(dead_code)]
    pub fn delete_older_than(&self, days: i64) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 
             AND created_at < datetime('now', '-' || ?1 || ' days')",
            params![days],
        )?;
        Ok(deleted as i64)
    }

    /// Helper to convert row to ClipboardItem
    fn row_to_item(row: &Row) -> Result<ClipboardItem, rusqlite::Error> {
        Ok(ClipboardItem {
            id: row.get("id")?,
            content_type: row.get("content_type")?,
            text_content: row.get("text_content")?,
            html_content: row.get("html_content")?,
            rtf_content: row.get("rtf_content")?,
            image_path: row.get("image_path")?,
            file_paths: row.get("file_paths")?,
            content_hash: row.get("content_hash")?,
            preview: row.get("preview")?,
            byte_size: row.get("byte_size")?,
            is_pinned: row.get("is_pinned")?,
            is_favorite: row.get("is_favorite")?,
            category_id: row.get("category_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            access_count: row.get("access_count")?,
            last_accessed_at: row.get("last_accessed_at")?,
        })
    }
}

/// Repository for categories
pub struct CategoryRepository {
    conn: Arc<Mutex<Connection>>,
}

impl CategoryRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Create a new category
    pub fn create(&self, name: &str, color: Option<&str>, icon: Option<&str>) -> Result<i64, rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO categories (name, color, icon) VALUES (?1, ?2, ?3)",
            params![name, color.unwrap_or("#6366f1"), icon.unwrap_or("folder")],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// List all categories
    pub fn list(&self) -> Result<Vec<Category>, rusqlite::Error> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT * FROM categories ORDER BY sort_order, name"
        )?;
        
        let categories = stmt
            .query_map([], |row| {
                Ok(Category {
                    id: row.get("id")?,
                    name: row.get("name")?,
                    color: row.get("color")?,
                    icon: row.get("icon")?,
                    sort_order: row.get("sort_order")?,
                    created_at: row.get("created_at")?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(categories)
    }

    /// Delete a category
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Repository for settings
pub struct SettingsRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SettingsRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            conn: db.connection(),
        }
    }

    /// Get a setting value
    pub fn get(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Set a setting value
    pub fn set(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get all settings
    pub fn get_all(&self) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let settings = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(settings)
    }
}
