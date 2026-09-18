use crate::database::Database;
use anyhow::{Result, bail};
use rusqlite::params;

/// Move records to the default group before deleting the group, avoiding the
/// legacy schema's ON DELETE CASCADE behavior.
pub fn delete_preserving_items(db: &Database, group_id: i64) -> Result<usize> {
    let connection = db.write_connection();
    let mut connection = connection.lock();
    let tx = connection.transaction()?;

    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM groups WHERE id = ?1)",
        params![group_id],
        |row| row.get(0),
    )?;
    if !exists {
        bail!("分组已不存在");
    }

    let item_ids: Vec<i64> = {
        let mut query = tx.prepare(
            "SELECT id FROM clipboard_items WHERE group_id = ?1
             ORDER BY is_pinned DESC, sort_order DESC, created_at DESC, id DESC",
        )?;
        query
            .query_map(params![group_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?
    };
    let favorite_ids: Vec<i64> = {
        let mut query = tx.prepare(
            "SELECT id FROM clipboard_items WHERE group_id = ?1 AND is_favorite = 1
             ORDER BY is_pinned DESC, favorite_order DESC, sort_order DESC, id DESC",
        )?;
        query
            .query_map(params![group_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?
    };

    let max_order: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) FROM clipboard_items WHERE group_id IS NULL",
        [],
        |row| row.get(0),
    )?;
    let max_favorite_order: i64 = tx.query_row(
        "SELECT COALESCE(MAX(favorite_order), 0) FROM clipboard_items
         WHERE group_id IS NULL AND is_favorite = 1",
        [],
        |row| row.get(0),
    )?;
    let item_count = i64::try_from(item_ids.len())?;
    let favorite_count = i64::try_from(favorite_ids.len())?;
    let next_order = max_order.max(0).checked_add(item_count);
    let next_favorite_order = max_favorite_order.max(0).checked_add(favorite_count);
    if next_order.is_none() || next_favorite_order.is_none() {
        bail!("记录排序值已达到上限，无法安全删除分组");
    }

    for (index, id) in item_ids.iter().enumerate() {
        let order = max_order.max(0) + item_count - i64::try_from(index)?;
        tx.execute(
            "UPDATE clipboard_items SET group_id = NULL, sort_order = ?1 WHERE id = ?2",
            params![order, id],
        )?;
    }
    for (index, id) in favorite_ids.iter().enumerate() {
        let order = max_favorite_order.max(0) + favorite_count - i64::try_from(index)?;
        tx.execute(
            "UPDATE clipboard_items SET favorite_order = ?1 WHERE id = ?2",
            params![order, id],
        )?;
    }
    tx.execute("DELETE FROM groups WHERE id = ?1", params![group_id])?;
    tx.commit()?;
    Ok(item_ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, database::NewClipboardItem};

    #[test]
    fn group_deletion_preserves_items_duplicates_and_order() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let default = history.capture("same text")?.unwrap();
        let first = history.capture("first")?.unwrap();
        let second = history.capture("second")?.unwrap();
        let group = history.create_group("工作")?;
        history.move_to_group(first, None, Some(group.id))?;
        history.move_to_group(second, None, Some(group.id))?;
        history.toggle_favorite(first)?;
        let duplicate = history.repo.insert(NewClipboardItem {
            text_content: Some("same text".into()),
            preview: Some("same text".into()),
            content_hash: history.item(default)?.content_hash.clone(),
            semantic_hash: history.item(default)?.semantic_hash.clone(),
            group_id: Some(group.id),
            ..Default::default()
        })?;
        assert_eq!(history.delete_group_preserving_items(group.id)?, 3);
        assert!(history.groups()?.is_empty());
        assert_eq!(history.item(first)?.group_id, None);
        assert_eq!(history.item(second)?.group_id, None);
        assert_eq!(history.item(duplicate)?.group_id, None);
        assert_eq!(history.item(default)?.group_id, None);
        assert!(history.item(first)?.is_favorite);
        let list = history.list("", 10, false)?;
        assert_eq!(list[0].id, duplicate);
        assert_eq!(list[1].id, second);
        assert_eq!(list[2].id, first);
        drop(history);
        assert_eq!(History::open(path)?.list("", 10, false)?.len(), 4);
        Ok(())
    }

    #[test]
    fn failed_group_deletion_rolls_back_moved_items() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let group = history.create_group("工作")?;
        let item = history.capture("keep")?.unwrap();
        history.move_to_group(item, None, Some(group.id))?;
        let connection = history.db.write_connection();
        connection.lock().execute_batch(
            "CREATE TRIGGER reject_group_move BEFORE UPDATE OF group_id ON clipboard_items
             BEGIN SELECT RAISE(ABORT, 'test rejection'); END;",
        )?;
        assert!(history.delete_group_preserving_items(group.id).is_err());
        assert_eq!(history.item(item)?.group_id, Some(group.id));
        assert_eq!(history.groups()?.len(), 1);
        assert!(history.delete_group_preserving_items(-1).is_err());
        Ok(())
    }
}
