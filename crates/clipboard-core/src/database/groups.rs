use super::{Database, Group};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use std::sync::Arc;
use tracing::debug;

/// 自定义分组仓库
pub struct GroupRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl GroupRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// 列出所有分组（含每个分组的条目数）
    pub fn list_with_count(&self) -> Result<Vec<Group>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT g.id, g.name, g.sort_order, g.created_at, \
             COUNT(ci.id) AS item_count \
             FROM groups g \
             LEFT JOIN clipboard_items ci ON ci.group_id = g.id \
             GROUP BY g.id \
             ORDER BY g.sort_order ASC, g.created_at ASC",
        )?;
        let groups = stmt
            .query_map([], |row| {
                Ok(Group {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    created_at: row.get(3)?,
                    item_count: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(groups)
    }

    /// 创建新分组，返回完整分组对象
    pub fn create(&self, name: &str) -> Result<Group, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let max_sort: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM groups",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        conn.execute(
            "INSERT INTO groups (name, sort_order) VALUES (?1, ?2)",
            params![name, max_sort + 1],
        )?;
        let id = conn.last_insert_rowid();
        let group = conn.query_row(
            "SELECT g.id, g.name, g.sort_order, g.created_at, 0 AS item_count FROM groups g WHERE g.id = ?1",
            params![id],
            |row| Ok(Group {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: row.get(3)?,
                item_count: row.get(4)?,
            }),
        )?;
        debug!("Created group: id={}, name={}", id, name);
        Ok(group)
    }

    /// 重命名分组
    pub fn rename(&self, id: i64, name: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE groups SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        debug!("Renamed group {} to {}", id, name);
        Ok(())
    }

    /// 删除分组（ON DELETE CASCADE 自动删除该分组的所有 clipboard_items）
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        debug!("Deleted group {}", id);
        Ok(())
    }

    /// 将条目移动到指定分组（None = 移回默认分组）
    pub fn move_item_to_group(
        &self,
        item_id: i64,
        group_id: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE clipboard_items SET group_id = ?1 WHERE id = ?2",
            params![group_id, item_id],
        )?;
        debug!("Moved item {} to group {:?}", item_id, group_id);
        Ok(())
    }
}
