//! 1.2.10 → 2.0 数据清理：检测旧 staged 副本与残留引用，由 UI 确认后执行。
use crate::History;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs, path::Path};

pub(crate) const CLEANUP_UI_DONE_KEY: &str = "_migration_v2_cleanup_ui_done";
pub(crate) const CLEANUP_UI_NEEDED_KEY: &str = "_migration_v2_cleanup_ui_needed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyMigrationStatus {
    /// 需要展示清理界面（尚未标记完成，且有升级痕迹或残留）。
    pub pending: bool,
    pub staged_bytes: u64,
    pub staged_count: u64,
    /// 仍含 `staged` 字段的 file_payload 行数。
    pub residual_payloads: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyMigrationReport {
    pub staged_bytes_removed: u64,
    pub staged_files_removed: u64,
    pub payloads_cleaned: u64,
}

impl History {
    /// 启动时调用：判断是否要弹出 2.0 数据清理界面。
    /// 纯 2.0 新库会在写连接上静默标记完成，避免每次启动误弹。
    pub fn legacy_migration_status(&self, data_dir: &Path) -> Result<LegacyMigrationStatus> {
        let writer = self.db.write_connection();
        let conn = writer.lock();
        let status = inspect_legacy_migration(&conn, data_dir)?;
        if !status.pending {
            let done_already: bool = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params![CLEANUP_UI_DONE_KEY],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .is_some();
            if !done_already {
                conn.execute(
                    "INSERT OR IGNORE INTO settings (key, value, updated_at)
                     VALUES (?1, '1', datetime('now', 'localtime'))",
                    params![CLEANUP_UI_DONE_KEY],
                )?;
            }
        }
        Ok(status)
    }

    /// 用户确认后：清引用、删 staged，并标记清理界面完成。
    pub fn complete_legacy_migration(&self, data_dir: &Path) -> Result<LegacyMigrationReport> {
        let before = {
            let writer = self.db.write_connection();
            inspect_legacy_migration(&writer.lock(), data_dir)?
        };
        self.remove_file_copies(data_dir)?;
        {
            let writer = self.db.write_connection();
            let conn = writer.lock();
            conn.execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES (?1, '1', datetime('now', 'localtime'))
                 ON CONFLICT(key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![CLEANUP_UI_DONE_KEY],
            )?;
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                params![CLEANUP_UI_NEEDED_KEY],
            )?;
        }
        Ok(LegacyMigrationReport {
            staged_bytes_removed: before.staged_bytes,
            staged_files_removed: before.staged_count,
            payloads_cleaned: before.residual_payloads,
        })
    }
}

/// 1.2.10 补 `note` 时打标，确保即使已无 staged 也会进入一次清理确认。
pub(crate) fn mark_cleanup_ui_needed(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at)
         VALUES (?1, '1', datetime('now', 'localtime'))
         ON CONFLICT(key) DO UPDATE SET
           value = excluded.value,
           updated_at = excluded.updated_at",
        params![CLEANUP_UI_NEEDED_KEY],
    )?;
    Ok(())
}

pub(crate) fn inspect_legacy_migration(
    conn: &Connection,
    data_dir: &Path,
) -> Result<LegacyMigrationStatus> {
    let done: bool = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![CLEANUP_UI_DONE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    let (staged_bytes, staged_count) = staged_tree_stats(data_dir)?;
    let residual_payloads = conn.query_row(
        "SELECT COUNT(*) FROM clipboard_items
         WHERE file_payload IS NOT NULL
           AND instr(file_payload, '\"staged\"') > 0",
        [],
        |row| row.get::<_, i64>(0),
    )? as u64;
    let needed: bool = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![CLEANUP_UI_NEEDED_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some();
    let has_residue = staged_count > 0 || residual_payloads > 0;
    if done {
        return Ok(LegacyMigrationStatus {
            pending: has_residue,
            staged_bytes,
            staged_count,
            residual_payloads,
        });
    }
    if !needed && !has_residue {
        return Ok(LegacyMigrationStatus {
            pending: false,
            staged_bytes: 0,
            staged_count: 0,
            residual_payloads: 0,
        });
    }
    Ok(LegacyMigrationStatus {
        pending: true,
        staged_bytes,
        staged_count,
        residual_payloads,
    })
}

fn staged_tree_stats(data_dir: &Path) -> Result<(u64, u64)> {
    let root = data_dir.join("staged");
    if !root.exists() {
        return Ok((0, 0));
    }
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut pending = vec![root];
    while let Some(dir) = pending.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("无法读取 {}", dir.display()));
            }
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                bytes = bytes
                    .checked_add(entry.metadata()?.len())
                    .context("staged 体积过大")?;
                count = count.checked_add(1).context("staged 文件数过多")?;
            }
        }
    }
    Ok((bytes, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use tempfile::tempdir;

    #[test]
    fn fresh_db_auto_marks_cleanup_done_without_pending_ui() -> Result<()> {
        let dir = tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let status = history.legacy_migration_status(dir.path())?;
        assert!(!status.pending);
        assert_eq!((status.staged_count, status.residual_payloads), (0, 0));
        let done: String = history.db.read_connection().lock().query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![CLEANUP_UI_DONE_KEY],
            |row| row.get(0),
        )?;
        assert_eq!(done, "1");
        Ok(())
    }

    #[test]
    fn staged_residue_keeps_pending_until_complete() -> Result<()> {
        let dir = tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let staged = dir.path().join("staged/hash");
        fs::create_dir_all(&staged)?;
        fs::write(staged.join("copy.bin"), b"old")?;
        // 清掉自动 done，模拟升级后首次检测。
        history.db.write_connection().lock().execute(
            "DELETE FROM settings WHERE key = ?1",
            params![CLEANUP_UI_DONE_KEY],
        )?;
        history.db.write_connection().lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, '1')",
            params![CLEANUP_UI_NEEDED_KEY],
        )?;
        let status = history.legacy_migration_status(dir.path())?;
        assert!(status.pending);
        assert_eq!(status.staged_count, 1);
        assert!(status.staged_bytes >= 3);
        let report = history.complete_legacy_migration(dir.path())?;
        assert_eq!(report.staged_files_removed, 1);
        assert!(!dir.path().join("staged").exists());
        assert!(!history.legacy_migration_status(dir.path())?.pending);
        Ok(())
    }

    #[test]
    fn note_migration_marks_cleanup_needed() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("legacy.db");
        {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch(crate::database::V1_2_10_SCHEMA)?;
        }
        let db = Database::new(path)?;
        let needed: i64 = db.write_connection().lock().query_row(
            "SELECT COUNT(*) FROM settings WHERE key = ?1",
            params![CLEANUP_UI_NEEDED_KEY],
            |row| row.get(0),
        )?;
        assert_eq!(needed, 1);
        let history = History::new(&db);
        let status = history.legacy_migration_status(dir.path())?;
        assert!(status.pending);
        Ok(())
    }
}
