use crate::database::Database;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::sync::Arc;

/// 设置仓库
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

    pub fn set(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, datetime('now', 'localtime'))",
            params![key, value],
        )?;
        Ok(())
    }

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

    /// 批量读取多个设置项，一次查询避免多次加锁和多次 SQL 往返
    pub fn get_multiple(&self, keys: &[&str]) -> Result<std::collections::HashMap<String, String>, rusqlite::Error> {
        if keys.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let conn = self.read_conn.lock();
        let placeholders: Vec<String> = (1..=keys.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT key, value FROM settings WHERE key IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = keys.iter().map(|k| k as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// 读取设置值，出错或不存在时返回默认值
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key)
            .ok()
            .flatten()
            .unwrap_or_else(|| default.to_string())
    }

    /// 读取布尔设置，出错或不存在时返回默认值
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.get(key)
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(default)
    }

    /// 清空所有设置
    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM settings", [])?;
        Ok(())
    }
}