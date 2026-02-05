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
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: i64,
    pub last_accessed_at: Option<String>,
    /// Whether all files in file_paths exist (only for "files" content_type)
    /// This field is computed at query time, not stored in database
    #[serde(default, skip_deserializing)]
    pub files_valid: Option<bool>,
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
    pub pinned_only: bool,
    pub favorite_only: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Repository for clipboard items
/// Uses read-write connection separation for better concurrency
pub struct ClipboardRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl ClipboardRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// Insert a new clipboard item
    pub fn insert(&self, item: NewClipboardItem) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        
        let file_paths_json = item.file_paths.map(|paths| serde_json::to_string(&paths).unwrap_or_default());
        
        // Get max sort_order and increment
        let max_sort_order: i64 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        let new_sort_order = max_sort_order + 1;
        
        conn.execute(
            "INSERT INTO clipboard_items (content_type, text_content, html_content, rtf_content, image_path, file_paths, content_hash, preview, byte_size, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                new_sort_order,
            ],
        )?;

        let id = conn.last_insert_rowid();
        debug!("Inserted clipboard item with id: {}, sort_order: {}", id, new_sort_order);
        Ok(id)
    }

    /// Check if item with hash exists
    pub fn exists_by_hash(&self, hash: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_hash = ?1",
            params![hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get item by hash and update access time
    pub fn touch_by_hash(&self, hash: &str) -> Result<Option<i64>, rusqlite::Error> {
        let conn = self.write_conn.lock();
        
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
        let conn = self.read_conn.lock();
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
        let conn = self.read_conn.lock();
        
        let mut sql = String::from(
            "SELECT clipboard_items.* FROM clipboard_items"
        );
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // Full-text search - escape special FTS characters
        if let Some(ref search) = options.search {
            if !search.is_empty() {
                sql = format!(
                    "SELECT clipboard_items.* FROM clipboard_items 
                     INNER JOIN clipboard_fts ON clipboard_items.id = clipboard_fts.rowid"
                );
                conditions.push("clipboard_fts MATCH ?".to_string());
                // Escape special FTS5 characters and add prefix matching
                let escaped_search = search
                    .replace('"', "\"\"")
                    .replace('*', "")
                    .replace('(', "")
                    .replace(')', "");
                params_vec.push(Box::new(format!("\"{}\"*", escaped_search)));
            }
        }

        // Filter by content type
        if let Some(ref content_type) = options.content_type {
            conditions.push("content_type = ?".to_string());
            params_vec.push(Box::new(content_type.clone()));
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

        // Order by: pinned first, then by sort_order (higher = newer), then by created_at
        sql.push_str(" ORDER BY is_pinned DESC, sort_order DESC, created_at DESC");

        // Limit and offset - use parameterized query
        sql.push_str(" LIMIT ? OFFSET ?");
        let limit = options.limit.unwrap_or(100);
        let offset = options.offset.unwrap_or(0);
        params_vec.push(Box::new(limit));
        params_vec.push(Box::new(offset));

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
        let conn = self.read_conn.lock();
        
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

    /// Toggle favorite status
    pub fn toggle_favorite(&self, id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.write_conn.lock();
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
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM clipboard_items WHERE id = ?1", params![id])?;
        debug!("Deleted clipboard item with id: {}", id);
        Ok(())
    }

    /// Get image paths of items that will be cleared (non-pinned, non-favorite)
    pub fn get_clearable_image_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL"
        )?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// Delete all non-pinned items
    pub fn clear_history(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
        )?;
        Ok(deleted as i64)
    }

    /// Delete items older than days
    #[allow(dead_code)]
    pub fn delete_older_than(&self, days: i64) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute(
            "DELETE FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 
             AND created_at < datetime('now', '-' || ?1 || ' days')",
            params![days],
        )?;
        Ok(deleted as i64)
    }

    /// Get total count of non-pinned, non-favorite items
    #[allow(dead_code)]
    pub fn get_non_protected_count(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Delete oldest non-pinned, non-favorite items to maintain max count
    /// Returns (deleted_count, image_paths_to_delete)
    pub fn enforce_max_count(&self, max_count: i64) -> Result<(i64, Vec<String>), rusqlite::Error> {
        if max_count <= 0 {
            // 0 means unlimited
            return Ok((0, vec![]));
        }

        let conn = self.write_conn.lock();
        
        // Get current count of non-protected items
        let current_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0",
            [],
            |row| row.get(0),
        )?;

        if current_count <= max_count {
            return Ok((0, vec![]));
        }

        let to_delete = current_count - max_count;
        
        // Get image paths of items to be deleted
        let mut stmt = conn.prepare(
            "SELECT image_path FROM clipboard_items 
             WHERE is_pinned = 0 AND is_favorite = 0 AND image_path IS NOT NULL
             ORDER BY created_at ASC 
             LIMIT ?1"
        )?;
        let image_paths: Vec<String> = stmt
            .query_map(params![to_delete], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        
        // Delete oldest non-protected items
        let deleted = conn.execute(
            "DELETE FROM clipboard_items WHERE id IN (
                SELECT id FROM clipboard_items 
                WHERE is_pinned = 0 AND is_favorite = 0 
                ORDER BY created_at ASC 
                LIMIT ?1
            )",
            params![to_delete],
        )?;

        debug!("Enforced max count: deleted {} oldest items", deleted);
        Ok((deleted as i64, image_paths))
    }

    /// Move item by swapping sort_order with target item
    pub fn move_item_by_id(&self, from_id: i64, to_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        
        // Get sort_order of both items
        let from_sort_order: i64 = conn.query_row(
            "SELECT sort_order FROM clipboard_items WHERE id = ?1",
            params![from_id],
            |row| row.get(0),
        )?;
        
        let to_sort_order: i64 = conn.query_row(
            "SELECT sort_order FROM clipboard_items WHERE id = ?1",
            params![to_id],
            |row| row.get(0),
        )?;
        
        // Swap sort_order values
        conn.execute(
            "UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2",
            params![to_sort_order, from_id],
        )?;
        
        conn.execute(
            "UPDATE clipboard_items SET sort_order = ?1 WHERE id = ?2",
            params![from_sort_order, to_id],
        )?;
        
        debug!("Moved item {} (sort_order: {} -> {}) with item {} (sort_order: {} -> {})",
            from_id, from_sort_order, to_sort_order,
            to_id, to_sort_order, from_sort_order);
        
        Ok(())
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
            sort_order: row.get("sort_order")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            access_count: row.get("access_count")?,
            last_accessed_at: row.get("last_accessed_at")?,
            files_valid: None, // Computed at query time, not stored in database
        })
    }
}

/// Repository for settings
/// Uses read-write connection separation for better concurrency
pub struct SettingsRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl SettingsRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// Get a setting value
    pub fn get(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
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
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get all settings
    pub fn get_all(&self) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
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
