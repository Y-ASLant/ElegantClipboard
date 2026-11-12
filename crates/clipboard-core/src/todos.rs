use crate::History;
use crate::database::Database;
use anyhow::{Result, bail};
use rusqlite::params;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub id: i64,
    pub text: String,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl History {
    pub fn list_todos(&self) -> Result<Vec<Todo>> {
        list(&self.db)
    }

    pub fn add_todo(&self, text: &str) -> Result<i64> {
        add(&self.db, text)
    }

    pub fn set_todo_completed(&self, id: i64, completed: bool) -> Result<()> {
        set_completed(&self.db, id, completed)
    }

    pub fn delete_todo(&self, id: i64) -> Result<()> {
        delete(&self.db, id)
    }

    pub fn update_todo(&self, id: i64, text: &str) -> Result<()> {
        update(&self.db, id, text)
    }

    pub fn reorder_todo(&self, from: i64, to: i64, after: bool) -> Result<()> {
        reorder(&self.db, from, to, after)
    }
}

fn list(db: &Database) -> Result<Vec<Todo>> {
    ensure_schema(db)?;
    let connection = db.read_connection();
    let connection = connection.lock();
    let mut query = connection.prepare(
        "SELECT id, text, completed, created_at, updated_at FROM todos
         ORDER BY completed ASC, sort_order DESC, id DESC",
    )?;
    let todos = query
        .query_map([], |row| {
            Ok(Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                completed: row.get::<_, i64>(2)? != 0,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(todos)
}

fn ensure_schema(db: &Database) -> Result<()> {
    let connection = db.write_connection();
    let connection = connection.lock();
    let has_updated_at: bool = connection.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('todos') WHERE name = 'updated_at'",
        [],
        |row| row.get(0),
    )?;
    if !has_updated_at {
        connection.execute_batch(
            "ALTER TABLE todos ADD COLUMN updated_at TEXT;
             UPDATE todos SET updated_at = created_at WHERE updated_at IS NULL;",
        )?;
    }
    let has_sort_order: bool = connection.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('todos') WHERE name = 'sort_order'",
        [],
        |row| row.get(0),
    )?;
    if !has_sort_order {
        connection.execute_batch(
            "ALTER TABLE todos ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
             UPDATE todos SET sort_order = id;",
        )?;
    }
    Ok(())
}

fn reorder(db: &Database, from: i64, to: i64, after: bool) -> Result<()> {
    if from == to {
        return Ok(());
    }
    ensure_schema(db)?;
    let connection = db.write_connection();
    let mut connection = connection.lock();
    let tx = connection.transaction()?;
    let completed: i64 = tx
        .query_row(
            "SELECT completed FROM todos WHERE id = ?1",
            params![from],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("待办已不存在"))?;
    let target_completed: i64 = tx
        .query_row(
            "SELECT completed FROM todos WHERE id = ?1",
            params![to],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("目标待办已不存在"))?;
    if completed != target_completed {
        bail!("不能在不同完成状态之间移动待办");
    }
    let mut ids: Vec<i64> = {
        let mut query = tx.prepare(
            "SELECT id FROM todos WHERE completed = ?1 ORDER BY sort_order DESC, id DESC",
        )?;
        query
            .query_map(params![completed], |row| row.get(0))?
            .collect::<Result<_, _>>()?
    };
    if !ids.contains(&from) || !ids.contains(&to) {
        bail!("待办顺序已变化，请重新拖动");
    }
    ids.retain(|id| *id != from);
    let index = ids.iter().position(|id| *id == to).unwrap() + usize::from(after);
    ids.insert(index, from);
    {
        let mut update = tx.prepare("UPDATE todos SET sort_order = ?1 WHERE id = ?2")?;
        for (index, id) in ids.iter().enumerate() {
            let order = i64::try_from(ids.len().saturating_sub(index))?;
            update.execute(params![order, id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn checked_text(text: &str) -> Result<&str> {
    let text = text.trim();
    if text.is_empty() {
        bail!("待办内容无效");
    }
    Ok(text)
}

fn add(db: &Database, text: &str) -> Result<i64> {
    let text = checked_text(text)?;
    ensure_schema(db)?;
    let connection = db.write_connection();
    let connection = connection.lock();
    let sort_order: i64 = connection.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM todos WHERE completed = 0",
        [],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO todos (text, updated_at, sort_order) VALUES (?1, datetime('now', 'localtime'), ?2)",
        params![text, sort_order],
    )?;
    Ok(connection.last_insert_rowid())
}

fn set_completed(db: &Database, id: i64, completed: bool) -> Result<()> {
    let connection = db.write_connection();
    let changed = connection.lock().execute(
        "UPDATE todos SET completed = ?1 WHERE id = ?2",
        params![i64::from(completed), id],
    )?;
    if changed == 0 {
        bail!("待办已不存在");
    }
    Ok(())
}

fn update(db: &Database, id: i64, text: &str) -> Result<()> {
    let text = checked_text(text)?;
    ensure_schema(db)?;
    let connection = db.write_connection();
    let changed = connection.lock().execute(
        "UPDATE todos SET text = ?1, updated_at = datetime('now', 'localtime') WHERE id = ?2",
        params![text, id],
    )?;
    if changed == 0 {
        bail!("待办已不存在");
    }
    Ok(())
}

fn delete(db: &Database, id: i64) -> Result<()> {
    let connection = db.write_connection();
    let changed = connection
        .lock()
        .execute("DELETE FROM todos WHERE id = ?1", params![id])?;
    if changed == 0 {
        bail!("待办已不存在");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::History;

    #[test]
    fn added_tasks_can_be_checked_and_sink_below_open_tasks() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("history.db"))?;
        let first = history.add_todo("  buy milk  ")?;
        let second = history.add_todo("write notes")?;
        history.set_todo_completed(first, true)?;
        let todos = history.list_todos()?;
        assert_eq!(todos[0].id, second);
        assert!(!todos[0].completed);
        assert_eq!(todos[1].text, "buy milk");
        assert!(todos[1].completed);
        history.update_todo(second, "**bold**\n- item")?;
        assert_eq!(history.list_todos()?[0].text, "**bold**\n- item");
        history.delete_todo(first)?;
        assert!(history.list_todos()?.iter().all(|todo| todo.id != first));
        assert!(history.add_todo("   ").is_err());
        Ok(())
    }

    #[test]
    fn open_tasks_keep_user_order_after_reorder() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("history.db"))?;
        let a = history.add_todo("first")?;
        let b = history.add_todo("second")?;
        let c = history.add_todo("third")?;
        assert_eq!(
            history
                .list_todos()?
                .into_iter()
                .take(3)
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            [c, b, a]
        );
        history.reorder_todo(a, b, false)?;
        let ids: Vec<_> = history
            .list_todos()?
            .into_iter()
            .take(3)
            .map(|todo| todo.id)
            .collect();
        assert_eq!(ids, [c, a, b]);
        Ok(())
    }
}
