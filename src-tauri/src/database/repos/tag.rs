use crate::database::repos::{TagAssocSyncEntry, TagSyncEntry, TagsSyncData};
use crate::database::Database;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub item_count: i64,
}

/// 标签仓库
pub struct TagRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

impl TagRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// 列出所有标签（含每个标签的条目数）
    pub fn list_with_count(&self) -> Result<Vec<Tag>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.sort_order, t.created_at, \
             COUNT(it.item_id) AS item_count \
             FROM tags t \
             LEFT JOIN item_tags it ON it.tag_id = t.id \
             GROUP BY t.id \
             ORDER BY t.sort_order ASC, t.created_at ASC",
        )?;
        let tags = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    created_at: row.get(3)?,
                    item_count: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// 创建新标签，返回完整标签对象
    pub fn create(&self, name: &str) -> Result<Tag, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let max_order: i64 = conn.query_row(
            "SELECT COALESCE(MAX(sort_order), 0) FROM tags",
            [],
            |row| row.get(0),
        ).unwrap_or(0);
        let sort_order = max_order + 1;
        conn.execute(
            "INSERT INTO tags (name, sort_order) VALUES (?1, ?2)",
            params![name, sort_order],
        )?;
        let id = conn.last_insert_rowid();
        let tag = conn.query_row(
            "SELECT id, name, sort_order, created_at FROM tags WHERE id = ?1",
            params![id],
            |row| Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_order: row.get(2)?,
                created_at: row.get(3)?,
                item_count: 0,
            }),
        )?;
        debug!("Created tag: id={}, name={}", id, name);
        Ok(tag)
    }

    /// 重命名标签
    pub fn rename(&self, id: i64, name: &str) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        debug!("Renamed tag {} to {}", id, name);
        Ok(())
    }

    /// 删除标签（item_tags 中的关联通过 ON DELETE CASCADE 自动删除）
    pub fn delete(&self, id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        debug!("Deleted tag {}", id);
        Ok(())
    }

    /// 为条目添加标签
    pub fn add_tag_to_item(&self, item_id: i64, tag_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
            params![item_id, tag_id],
        )?;
        debug!("Added tag {} to item {}", tag_id, item_id);
        Ok(())
    }

    /// 从条目移除标签
    pub fn remove_tag_from_item(&self, item_id: i64, tag_id: i64) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
            params![item_id, tag_id],
        )?;
        debug!("Removed tag {} from item {}", tag_id, item_id);
        Ok(())
    }

    /// 判断条目是否仍关联任意标签。
    pub fn has_tags_for_item(&self, item_id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.read_conn.lock();
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM item_tags WHERE item_id = ?1 LIMIT 1)",
            params![item_id],
            |row| row.get(0),
        )
    }

    /// 获取条目的所有标签
    pub fn get_tags_for_item(&self, item_id: i64) -> Result<Vec<Tag>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.sort_order, t.created_at, 0 AS item_count \
             FROM tags t \
             INNER JOIN item_tags it ON it.tag_id = t.id \
             WHERE it.item_id = ?1 \
             ORDER BY t.sort_order ASC",
        )?;
        let tags = stmt
            .query_map(params![item_id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    created_at: row.get(3)?,
                    item_count: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    /// 批量更新标签内条目的排序
    pub fn reorder_tag_items(&self, tag_id: i64, item_ids: &[i64]) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut stmt = conn.prepare(
            "UPDATE item_tags SET sort_order = ?1 WHERE tag_id = ?2 AND item_id = ?3",
        )?;
        for (i, item_id) in item_ids.iter().enumerate() {
            stmt.execute(params![i as i64, tag_id, item_id])?;
        }
        debug!("Reordered {} items in tag {}", item_ids.len(), tag_id);
        Ok(())
    }

    /// 批量更新标签排序
    pub fn reorder_tags(&self, tag_ids: &[i64]) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut stmt = conn.prepare("UPDATE tags SET sort_order = ?1 WHERE id = ?2")?;
        for (i, id) in tag_ids.iter().enumerate() {
            stmt.execute(params![i as i64, id])?;
        }
        debug!("Reordered {} tags", tag_ids.len());
        Ok(())
    }

    /// 导出标签及其条目关联（使用 content_hash 而非 item_id，以便跨设备匹配）
    pub fn export_tags_sync_data(&self) -> Result<TagsSyncData, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn.prepare(
            "SELECT name, sort_order, created_at FROM tags ORDER BY sort_order ASC",
        )?;
        let tags: Vec<TagSyncEntry> = stmt
            .query_map([], |row| {
                Ok(TagSyncEntry {
                    name: row.get(0)?,
                    sort_order: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut stmt = conn.prepare(
            "SELECT ci.content_hash, t.name, it.sort_order \
             FROM item_tags it \
             INNER JOIN clipboard_items ci ON ci.id = it.item_id \
             INNER JOIN tags t ON t.id = it.tag_id",
        )?;
        let associations: Vec<TagAssocSyncEntry> = stmt
            .query_map([], |row| {
                Ok(TagAssocSyncEntry {
                    content_hash: row.get(0)?,
                    tag_name: row.get(1)?,
                    sort_order: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(TagsSyncData { tags, associations })
    }

    /// 导入标签及其条目关联
    pub fn import_tags_sync_data(&self, data: &TagsSyncData) -> Result<usize, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let mut count = 0usize;

        for tag in &data.tags {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM tags WHERE name = ?1",
                    params![tag.name],
                    |row| row.get(0),
                )
                .unwrap_or(true);

            if !exists {
                conn.execute(
                    "INSERT INTO tags (name, sort_order, created_at) VALUES (?1, ?2, ?3)",
                    params![tag.name, tag.sort_order, tag.created_at],
                )?;
                count += 1;
            } else {
                conn.execute(
                    "UPDATE tags SET sort_order = ?1 WHERE name = ?2",
                    params![tag.sort_order, tag.name],
                )?;
            }
        }

        let mut assoc_count = 0usize;
        for assoc in &data.associations {
            let item_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM clipboard_items WHERE content_hash = ?1 LIMIT 1",
                    params![assoc.content_hash],
                    |row| row.get(0),
                )
                .ok();

            let tag_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM tags WHERE name = ?1",
                    params![assoc.tag_name],
                    |row| row.get(0),
                )
                .ok();

            if let (Some(item_id), Some(tag_id)) = (item_id, tag_id) {
                let inserted = conn.execute(
                    "INSERT OR IGNORE INTO item_tags (item_id, tag_id, sort_order) VALUES (?1, ?2, ?3)",
                    params![item_id, tag_id, assoc.sort_order],
                )?;
                if inserted > 0 {
                    assoc_count += 1;
                } else {
                    conn.execute(
                        "UPDATE item_tags SET sort_order = ?1 WHERE item_id = ?2 AND tag_id = ?3",
                        params![assoc.sort_order, item_id, tag_id],
                    )?;
                }
            }
        }

        debug!("Imported {} tags, {} associations", count, assoc_count);
        Ok(count)
    }
}