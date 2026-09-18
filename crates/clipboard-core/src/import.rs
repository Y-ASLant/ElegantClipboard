use crate::database::Database;
use anyhow::{Context, Result, bail};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub total_items: i64,
    pub visible_text_items: i64,
}

/// Staged databases are installed as a single file, without their WAL sidecars.
pub(crate) fn checkpoint_staged_database(connection: &Connection) -> Result<()> {
    let busy: i64 =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
    if busy != 0 {
        bail!("暂存数据库仍有未合并的日志");
    }
    Ok(())
}

/// Import a consistent snapshot without changing the legacy database.
/// The caller must hold the destination instance lock throughout this call.
pub fn import_legacy_database(source: &Path, destination: &Path) -> Result<ImportReport> {
    if destination.exists() {
        bail!("GPUI 数据库已存在，导入不会覆盖现有历史");
    }
    if !source.is_file() {
        bail!("旧版数据库不存在：{}", source.display());
    }
    let parent = destination.parent().context("GPUI 数据库路径缺少父目录")?;
    std::fs::create_dir_all(parent).context("无法创建 GPUI 数据目录")?;
    let source_db = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("无法只读打开旧版数据库")?;
    let valid: bool = source_db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'clipboard_items')",
        [],
        |row| row.get(0),
    )?;
    if !valid {
        bail!("所选文件不是 ElegantClipboard 历史数据库");
    }
    let temporary = tempfile::Builder::new()
        .prefix(".clipboard-import-")
        .tempdir_in(parent)
        .context("无法创建导入暂存目录")?;
    let staged = temporary.path().join("clipboard.db");
    {
        let mut staged_db = Connection::open(&staged)?;
        let backup = Backup::new(&source_db, &mut staged_db)?;
        let mut last_progress = Instant::now();
        loop {
            match backup.step(64).context("备份旧版数据库失败")? {
                StepResult::Done => break,
                StepResult::More => last_progress = Instant::now(),
                StepResult::Busy | StepResult::Locked => {
                    if last_progress.elapsed() > Duration::from_secs(10) {
                        bail!("旧版数据库持续被占用，导入已中止");
                    }
                }
                _ => bail!("旧版数据库备份未完成"),
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    drop(source_db);
    let report = {
        let database = Database::new(staged.clone()).context("旧版数据库升级失败")?;
        database
            .write_connection()
            .lock()
            .execute(crate::preferences::PRUNE_NON_GPUI_SETTINGS_SQL, [])?;
        let report = {
            let reader = database.read_connection();
            let connection = reader.lock();
            let integrity: String =
                connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                bail!("导入副本校验失败：{integrity}");
            }
            ImportReport {
                total_items: connection.query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| {
                    row.get(0)
                })?,
                visible_text_items: connection.query_row(
                    "SELECT COUNT(*) FROM clipboard_items WHERE content_type IN ('text', 'url') AND group_id IS NULL",
                    [],
                    |row| row.get(0),
                )?,
            }
        };
        checkpoint_staged_database(&database.write_connection().lock())?;
        report
    };
    std::fs::rename(&staged, destination).context("无法将导入副本移入 GPUI 数据目录")?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, PAGE_SIZE};

    #[test]
    fn imports_live_wal_and_preserves_history_metadata() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source_path = directory.path().join("old/clipboard.db");
        let destination = directory.path().join("new/clipboard.db");
        let old = Database::new(source_path.clone())?;
        let history = History::new(&old);
        let id = history.capture("原来的收藏文本")?.unwrap();
        history.toggle_favorite(id)?;
        history.toggle_pin(id)?;
        old.write_connection().lock().execute_batch(
            "INSERT INTO settings (key, value) VALUES
             ('secret_token', 'legacy-secret'),
             ('gpui_theme_mode', 'dark')",
        )?;
        old.write_connection().lock().execute(
            "INSERT INTO clipboard_items (content_type, content_hash, semantic_hash, image_path) VALUES ('image', 'image-hash', 'image-hash', 'C:/old/image.png')",
            [],
        )?;
        let report = import_legacy_database(&source_path, &destination)?;
        assert_eq!(report.total_items, 2);
        assert_eq!(report.visible_text_items, 1);
        assert_eq!(history.count("", false)?, 2);
        let imported = History::open(destination.clone())?;
        assert_eq!(imported.text(id)?, "原来的收藏文本");
        let items = imported.list("", PAGE_SIZE, true)?;
        assert_eq!(items.len(), 1);
        assert!(items[0].is_pinned);
        assert!(items[0].is_favorite);
        let imported_db = Database::new(destination.clone())?;
        let settings = imported_db.read_connection();
        let settings = settings.lock();
        let secret_count: i64 = settings.query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
            [],
            |row| row.get(0),
        )?;
        let theme: String = settings.query_row(
            "SELECT value FROM settings WHERE key = 'gpui_theme_mode'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(secret_count, 0);
        assert_eq!(theme, "dark");
        let source_secret_count: i64 = old.write_connection().lock().query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(source_secret_count, 1);
        assert!(import_legacy_database(&source_path, &destination).is_err());
        assert_eq!(imported.count("", false)?, 2);
        Ok(())
    }

    #[test]
    fn rejects_wrong_source_without_creating_database() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("wrong.db");
        let destination = directory.path().join("new/clipboard.db");
        Connection::open(&source)?;
        assert!(import_legacy_database(&source, &destination).is_err());
        assert!(!destination.exists());
        Ok(())
    }

    #[test]
    fn checkpoint_rejects_locked_wal_and_recovers_after_reader_exits() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("staged.db");
        let writer = Connection::open(&path)?;
        writer.busy_timeout(Duration::from_millis(50))?;
        writer.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE records (id INTEGER);
             INSERT INTO records VALUES (1);",
        )?;
        let reader = Connection::open(&path)?;
        reader.execute_batch("BEGIN; SELECT id FROM records")?;
        writer.execute("INSERT INTO records VALUES (2)", [])?;
        assert!(checkpoint_staged_database(&writer).is_err());
        reader.execute_batch("ROLLBACK")?;
        checkpoint_staged_database(&writer)?;
        Ok(())
    }
}
