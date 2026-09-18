use crate::database::Database;
use anyhow::{Result, bail};
use rusqlite::params;

/// Move relative to a stable ID, including records outside the loaded/search window.
pub fn move_item(db: &Database, from: i64, to: i64, after: bool, favorites: bool) -> Result<()> {
    let connection = db.write_connection();
    let mut connection = connection.lock();
    let tx = connection.transaction()?;
    let column = if favorites {
        "favorite_order"
    } else {
        "sort_order"
    };
    let filter = if favorites { "AND is_favorite = 1" } else { "" };
    let mut rows: Vec<(i64, bool)> = {
        let mut query = tx.prepare(&format!(
            "SELECT id, is_pinned FROM clipboard_items WHERE content_type = 'text' {filter}
             ORDER BY is_pinned DESC, {column} DESC, sort_order DESC, created_at DESC, id DESC"
        ))?;
        query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    let Some(&(_, pinned)) = rows.iter().find(|(id, _)| *id == from) else {
        bail!("拖动的记录已不存在或已不属于当前视图");
    };
    let Some(&(_, target_pinned)) = rows.iter().find(|(id, _)| *id == to) else {
        bail!("目标记录已不存在或已不属于当前视图");
    };
    if pinned != target_pinned {
        bail!("置顶与普通记录分别排序，请先调整置顶状态");
    }
    if from == to {
        return Ok(());
    }
    rows.retain(|(id, is_pinned)| *is_pinned == pinned && *id != from);
    let index = rows.iter().position(|(id, _)| *id == to).unwrap() + usize::from(after);
    rows.insert(index, (from, pinned));
    {
        let mut update = tx.prepare(&format!(
            "UPDATE clipboard_items SET {column} = ?1 WHERE id = ?2"
        ))?;
        for (index, (id, _)) in rows.iter().enumerate() {
            update.execute(params![(rows.len() - index) as i64, id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, PAGE_SIZE};

    #[test]
    fn reorder_covers_unloaded_rows_and_rolls_back_failed_writes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("history.db"))?;
        for i in 0..105 {
            history.capture(&format!("record {i}"))?;
        }
        let all = history.list("", 200, false)?;
        let from = all[0].id;
        let to = all[104].id;
        history.reorder(from, to, true, false)?;
        let reordered = history.list("", 200, false)?;
        assert_eq!(reordered.last().unwrap().id, from);
        assert_eq!(reordered[0].id, all[1].id);
        let connection = history.db.write_connection();
        connection.lock().execute_batch(&format!(
            "CREATE TRIGGER reject_reorder BEFORE UPDATE OF sort_order ON clipboard_items
             WHEN OLD.id = {to} BEGIN SELECT RAISE(ABORT, 'test write failure'); END;"
        ))?;
        assert!(
            history
                .reorder(from, reordered[0].id, false, false)
                .is_err()
        );
        let actual: Vec<_> = history
            .list("", 200, false)?
            .iter()
            .map(|item| (item.id, item.sort_order))
            .collect();
        assert_eq!(
            actual,
            reordered
                .iter()
                .map(|item| (item.id, item.sort_order))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn insertion_persists_and_preserves_unloaded_and_filtered_neighbors() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("history.db");
        let history = History::open(path.clone())?;
        let a = history.capture("match a")?.unwrap();
        let b = history.capture("hidden b")?.unwrap();
        let c = history.capture("match c")?.unwrap();
        let d = history.capture("match d")?.unwrap();
        history.reorder(a, d, false, false)?;
        let ids = |h: &History, favorites| -> Result<Vec<i64>> {
            Ok(h.list("", PAGE_SIZE, favorites)?
                .iter()
                .map(|item| item.id)
                .collect())
        };
        assert_eq!(ids(&history, false)?, vec![a, d, c, b]);
        history.reorder(a, b, true, false)?;
        assert_eq!(ids(&history, false)?, vec![d, c, b, a]);
        for id in [a, c, d] {
            history.toggle_favorite(id)?;
        }
        history.reorder(a, d, false, true)?;
        assert_eq!(ids(&history, true)?, vec![a, d, c]);
        assert_eq!(ids(&history, false)?, vec![d, c, b, a]);
        history.toggle_pin(d)?;
        let before = ids(&history, false)?;
        assert!(history.reorder(a, d, false, false).is_err());
        assert!(history.reorder(a, -1, false, false).is_err());
        assert!(history.reorder(b, c, false, true).is_err());
        assert_eq!(ids(&history, false)?, before);
        drop(history);
        assert_eq!(ids(&History::open(path)?, false)?, before);
        Ok(())
    }
}
