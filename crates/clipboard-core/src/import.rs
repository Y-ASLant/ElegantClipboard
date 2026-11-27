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
    import_legacy_database_with_progress(source, destination, &mut |_, _| {})
}

/// `progress(percent 0..=100, label)` 仅覆盖本阶段内部进度；调用方负责映射到总进度。
pub fn import_legacy_database_with_progress(
    source: &Path,
    destination: &Path,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<ImportReport> {
    if destination.exists() {
        bail!("GPUI 数据库已存在，导入不会覆盖现有历史");
    }
    if !source.is_file() {
        bail!("旧版数据库不存在：{}", source.display());
    }
    let parent = destination.parent().context("GPUI 数据库路径缺少父目录")?;
    std::fs::create_dir_all(parent).context("无法创建 GPUI 数据目录")?;
    progress(1.0, "读取旧版数据库");
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
        let mut last_reported = -1i32;
        loop {
            match backup.step(64).context("备份旧版数据库失败")? {
                StepResult::Done => break,
                StepResult::More => {
                    last_progress = Instant::now();
                    let snap = backup.progress();
                    let copy_pct = if snap.pagecount > 0 {
                        ((snap.pagecount - snap.remaining) as f32 / snap.pagecount as f32) * 100.0
                    } else {
                        0.0
                    };
                    let pct = 5.0 + copy_pct * 0.65; // 5..=70
                    let bucket = (pct / 2.0).floor() as i32;
                    if bucket != last_reported {
                        last_reported = bucket;
                        progress(pct, "复制数据库页");
                    }
                }
                StepResult::Busy | StepResult::Locked => {
                    if last_progress.elapsed() > Duration::from_secs(10) {
                        bail!("旧版数据库持续被占用，导入已中止");
                    }
                }
                _ => bail!("旧版数据库备份未完成"),
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        progress(70.0, "数据库复制完成");
    }
    drop(source_db);
    let report = {
        progress(72.0, "升级数据库结构");
        let database = Database::new(staged.clone()).context("旧版数据库升级失败")?;
        // 旧版导入只带走历史数据：用户设置一律丢弃，保留 _migration_* 标记。
        crate::database::SettingsRepository::new(&database).clear_user_settings()?;
        // 避免「未保存保留策略时新采集默认 10000 条」把导入的历史裁掉。
        crate::History::new(&database).preserve_imported_history_limits()?;
        crate::file_copy_cleanup::clear_file_copy_references(
            &mut database.write_connection().lock(),
        )?;
        let report = {
            let reader = database.read_connection();
            let connection = reader.lock();
            ImportReport {
                total_items: crate::backup_common::verify_staged_database(
                    &connection,
                    "导入副本校验失败",
                )?,
            }
        };
        checkpoint_staged_database(&database.write_connection().lock())?;
        report
    };
    progress(95.0, "写入目标数据库");
    std::fs::rename(&staged, destination).context("无法将导入副本移入 GPUI 数据目录")?;
    progress(100.0, "数据库就绪");
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
        std::fs::create_dir_all(source_path.parent().unwrap())?;
        let old = Connection::open(&source_path)?;
        old.execute_batch("PRAGMA journal_mode = WAL")?;
        old.execute_batch(crate::database::V1_2_10_SCHEMA)?;
        let id = old.query_row(
            "INSERT INTO clipboard_items (content_type, text_content, preview, content_hash,
                semantic_hash, is_favorite, is_pinned) VALUES ('text', ?1, ?1, 'a', 'a', 1, 1)
             RETURNING id",
            ["原来的收藏文本"],
            |r| r.get::<_, i64>(0),
        )?;
        old.execute_batch(
            "INSERT INTO settings (key, value) VALUES
             ('secret_token', 'legacy-secret'),
             ('gpui_theme_mode', 'dark')",
        )?;
        old.execute(
            "INSERT INTO clipboard_items (content_type, content_hash, semantic_hash, image_path) VALUES ('image', 'image-hash', 'image-hash', 'C:/old/image.png')",
            [],
        )?;
        let report = import_legacy_database(&source_path, &destination)?;
        assert_eq!(report.total_items, 2);
        let source_rows: i64 =
            old.query_row("SELECT COUNT(*) FROM clipboard_items", [], |r| r.get(0))?;
        assert_eq!(source_rows, 2);
        let imported = History::open(destination.clone())?;
        assert_eq!(imported.text(id)?, "原来的收藏文本");
        let items = imported.list("", PAGE_SIZE, true)?;
        assert_eq!(items.len(), 1);
        assert!(items[0].is_pinned);
        assert!(items[0].is_favorite_pinned);
        assert!(items[0].is_favorite);
        let imported_db = Database::new(destination.clone())?;
        let settings = imported_db.read_connection();
        let settings = settings.lock();
        let secret_count: i64 = settings.query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
            [],
            |row| row.get(0),
        )?;
        let theme_count: i64 = settings.query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'gpui_theme_mode'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(secret_count, 0);
        assert_eq!(theme_count, 0);
        // Database::new 会 INSERT OR IGNORE 产品默认 settings；断言旧库私货不在。
        let imported_keys: Vec<String> = {
            let mut statement = settings.prepare("SELECT key FROM settings")?;
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        assert!(
            !imported_keys.iter().any(|key| key == "secret_token"),
            "legacy secret must not import: {imported_keys:?}"
        );
        assert!(
            !imported_keys.iter().any(|key| key == "gpui_theme_mode"),
            "legacy/gpui preference rows must not import: {imported_keys:?}"
        );
        let source_secret_count: i64 = old.query_row(
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
    fn importing_legacy_database_drops_copy_references_without_modifying_source_assets()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source_dir = directory.path().join("old");
        std::fs::create_dir_all(source_dir.join("staged"))?;
        let source_path = source_dir.join("clipboard.db");
        let history = History::open(source_path.clone())?;
        let original = directory.path().join("original.txt");
        std::fs::write(&original, b"original")?;
        let copied = source_dir.join("staged/copy.txt");
        std::fs::write(&copied, b"retired copy")?;
        let id = history.capture_files(
            &[original.to_string_lossy().into_owned()],
            &source_dir.join("images"),
        )?;
        let payload =
            serde_json::json!({"staged":[{"original":original,"staged":copied}],"legacy":"keep"})
                .to_string();
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            rusqlite::params![payload, id],
        )?;
        let destination = directory.path().join("new/clipboard.db");
        import_legacy_database(&source_path, &destination)?;
        let restored = History::open(destination)?;
        let item = restored.item(id)?;
        let cleaned: serde_json::Value = serde_json::from_str(&item.file_payload.unwrap())?;
        assert!(cleaned.get("staged").is_none());
        assert_eq!(cleaned["legacy"], "keep");
        assert_eq!(
            restored.files(id)?,
            vec![original.to_string_lossy().into_owned()]
        );
        assert_eq!(
            history.item(id)?.file_payload.as_deref(),
            Some(payload.as_str())
        );
        assert_eq!(std::fs::read(copied)?, b"retired copy");
        assert_eq!(std::fs::read(original)?, b"original");
        assert!(!directory.path().join("new/staged").exists());
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
