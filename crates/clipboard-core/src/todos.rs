use crate::History;
use crate::database::Database;
use anyhow::{Result, bail};
use rusqlite::params;

/// 待办进度：手动设置，不自动流转。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TodoStatus {
    #[default]
    Pending = 0,
    InProgress = 1,
    Done = 2,
    Closed = 3,
}

impl TodoStatus {
    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Pending),
            1 => Some(Self::InProgress),
            2 => Some(Self::Done),
            3 => Some(Self::Closed),
            _ => None,
        }
    }

    pub fn as_i64(self) -> i64 {
        self as i64
    }

    /// 已处理 / 已关闭：列表可按设置隐藏，标题划线。
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Closed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub id: i64,
    pub text: String,
    pub status: TodoStatus,
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

    pub fn set_todo_status(&self, id: i64, status: TodoStatus) -> Result<()> {
        set_status(&self.db, id, status)
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
        "SELECT id, text, status, created_at, updated_at FROM todos
         ORDER BY status ASC, sort_order DESC, id DESC",
    )?;
    let todos = query
        .query_map([], |row| {
            let status = TodoStatus::from_i64(row.get(2)?).unwrap_or(TodoStatus::Pending);
            Ok(Todo {
                id: row.get(0)?,
                text: row.get(1)?,
                status,
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
    let has_status: bool = connection.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('todos') WHERE name = 'status'",
        [],
        |row| row.get(0),
    )?;
    if !has_status {
        // 旧 completed：0 → 待处理，非 0 → 已处理。
        connection.execute_batch(
            "ALTER TABLE todos ADD COLUMN status INTEGER NOT NULL DEFAULT 0;
             UPDATE todos SET status = CASE WHEN completed != 0 THEN 2 ELSE 0 END;",
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
    let status: i64 = tx
        .query_row(
            "SELECT status FROM todos WHERE id = ?1",
            params![from],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("待办已不存在"))?;
    let target_status: i64 = tx
        .query_row(
            "SELECT status FROM todos WHERE id = ?1",
            params![to],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("目标待办已不存在"))?;
    if status != target_status {
        bail!("不能在不同进度状态之间移动待办");
    }
    let mut ids: Vec<i64> = {
        let mut query =
            tx.prepare("SELECT id FROM todos WHERE status = ?1 ORDER BY sort_order DESC, id DESC")?;
        query
            .query_map(params![status], |row| row.get(0))?
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
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM todos WHERE status = 0",
        [],
        |row| row.get(0),
    )?;
    connection.execute(
        "INSERT INTO todos (text, status, updated_at, sort_order)
         VALUES (?1, 0, datetime('now', 'localtime'), ?2)",
        params![text, sort_order],
    )?;
    Ok(connection.last_insert_rowid())
}

fn set_status(db: &Database, id: i64, status: TodoStatus) -> Result<()> {
    ensure_schema(db)?;
    let connection = db.write_connection();
    let connection = connection.lock();
    let sort_order: i64 = connection.query_row(
        "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM todos WHERE status = ?1 AND id != ?2",
        params![status.as_i64(), id],
        |row| row.get(0),
    )?;
    let changed = connection.execute(
        "UPDATE todos
         SET status = ?1,
             sort_order = ?2,
             updated_at = datetime('now', 'localtime')
         WHERE id = ?3",
        params![status.as_i64(), sort_order, id],
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
    fn status_migration_and_manual_status_changes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("history.db"))?;
        let first = history.add_todo("  buy milk  ")?;
        let second = history.add_todo("write notes")?;
        assert_eq!(history.list_todos()?[0].status, TodoStatus::Pending);
        history.set_todo_status(first, TodoStatus::Done)?;
        let todos = history.list_todos()?;
        assert_eq!(todos[0].id, second);
        assert_eq!(todos[0].status, TodoStatus::Pending);
        assert_eq!(todos[1].id, first);
        assert_eq!(todos[1].status, TodoStatus::Done);
        history.set_todo_status(second, TodoStatus::InProgress)?;
        history.set_todo_status(first, TodoStatus::Closed)?;
        let todos = history.list_todos()?;
        assert_eq!(todos[0].status, TodoStatus::InProgress);
        assert_eq!(todos[1].status, TodoStatus::Closed);
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
        history.set_todo_status(a, TodoStatus::Done)?;
        assert!(history.reorder_todo(a, b, false).is_err());
        Ok(())
    }

    #[test]
    fn legacy_completed_column_maps_into_status() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("history.db");
        {
            let db = Database::new(path.clone())?;
            let conn = db.write_connection();
            conn.lock().execute_batch(
                "DROP TABLE IF EXISTS todos;
                 CREATE TABLE todos (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    text TEXT NOT NULL,
                    completed INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );
                 INSERT INTO todos (text, completed) VALUES ('open', 0), ('done', 1);",
            )?;
        }
        let history = History::open(path)?;
        let todos = history.list_todos()?;
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].text, "open");
        assert_eq!(todos[0].status, TodoStatus::Pending);
        assert_eq!(todos[1].text, "done");
        assert_eq!(todos[1].status, TodoStatus::Done);
        Ok(())
    }
}
