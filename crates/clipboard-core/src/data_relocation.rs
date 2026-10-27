//! 在调用方持锁移动整个数据目录后，修正库内的受管路径引用。
mod json;
mod paths;

use crate::{backup_common::verify_staged_database, database::Database};
use anyhow::{Context, Result, ensure};
use paths::Relocation;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

/// 将 `previous_roots` 内的绝对路径重定位到现有的 `data_dir`。
///
/// 调用方须全程阻止并发实例、已移动完整目录，并保留恢复标记直到本函数成功。
/// 旧根须同时包含原始路径与移动前的规范路径；已不存在的旧目录无法再规范化。
/// 本函数不读取、删除附件，也不要求附件存在。正常开库升级 schema 后，
/// 所有路径修改在一个事务中完成。成功或失败返回前均关闭自有数据库连接。
pub fn relocate_managed_paths(data_dir: &Path, previous_roots: &[PathBuf]) -> Result<()> {
    let relocation = Relocation::new(data_dir, previous_roots)?;
    let database_path = data_dir.join("clipboard.db");
    ensure!(database_path.is_file(), "迁移数据库不存在");
    {
        let connection =
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .context("无法校验迁移数据库")?;
        let valid: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table'
             AND name IN ('clipboard_items', 'clipboard_items_new'))",
            [],
            |row| row.get(0),
        )?;
        ensure!(valid, "迁移源不是剪贴板历史数据库");
        check_integrity(&connection)?;
    }
    let database = Database::new(database_path).context("迁移数据库升级失败")?;
    let writer = database.write_connection();
    let (result, checkpoint) = {
        let mut connection = writer.lock();
        let result = relocate_rows(&mut connection, &relocation);
        // 回滚后也合并日志，便于调用方将整个目录重命名回原位置。
        let checkpoint = crate::import::checkpoint_staged_database(&connection);
        (result, checkpoint)
    };
    drop(writer);
    drop(database);
    result?;
    checkpoint.context("迁移数据库日志合并失败")
}

fn check_integrity(connection: &Connection) -> Result<()> {
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    ensure!(integrity == "ok", "迁移数据库校验失败：{integrity}");
    let mut foreign_keys = connection.prepare("PRAGMA foreign_key_check")?;
    ensure!(
        foreign_keys.query([])?.next()?.is_none(),
        "迁移数据库存在无效的外键引用"
    );
    Ok(())
}

struct ItemPaths {
    id: i64,
    image: Option<String>,
    icon: Option<String>,
    files: Option<String>,
    payload: Option<String>,
}

fn relocate_rows(connection: &mut Connection, relocation: &Relocation) -> Result<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let count = verify_staged_database(&tx, "迁移数据库校验失败")?;
    // 再次 UPDATE 来恢复 updated_at 会重新触发时间戳更新。
    // 触发器修改受事务保护；错误或崩溃回滚时也会恢复原定义。
    let timestamp_trigger: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger'
             AND name = 'clipboard_items_update_timestamp'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    tx.execute_batch("DROP TRIGGER IF EXISTS clipboard_items_update_timestamp;")?;
    let items = {
        let mut query = tx.prepare(
            "SELECT id, image_path, source_app_icon, file_paths, file_payload
             FROM clipboard_items ORDER BY id",
        )?;
        query
            .query_map([], |row| {
                Ok(ItemPaths {
                    id: row.get(0)?,
                    image: row.get(1)?,
                    icon: row.get(2)?,
                    files: row.get(3)?,
                    payload: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for item in items {
        let remap = || -> Result<()> {
            let image = item.image.as_deref().and_then(|raw| relocation.map(raw));
            let icon = item.icon.as_deref().and_then(|raw| relocation.map(raw));
            let files = item
                .files
                .as_deref()
                .map(|raw| json::remap(raw, false, relocation))
                .transpose()?
                .flatten();
            let payload = item
                .payload
                .as_deref()
                .map(|raw| json::remap(raw, true, relocation))
                .transpose()?
                .flatten();
            if image.is_some() || icon.is_some() || files.is_some() || payload.is_some() {
                tx.execute(
                    "UPDATE clipboard_items SET image_path = ?1, source_app_icon = ?2,
                     file_paths = ?3, file_payload = ?4 WHERE id = ?5",
                    params![
                        image.as_ref().or(item.image.as_ref()),
                        icon.as_ref().or(item.icon.as_ref()),
                        files.as_ref().or(item.files.as_ref()),
                        payload.as_ref().or(item.payload.as_ref()),
                        item.id
                    ],
                )?;
            }
            Ok(())
        };
        remap().with_context(|| format!("重定位历史记录 {} 失败", item.id))?;
    }
    if let Some(sql) = timestamp_trigger {
        tx.execute_batch(&sql)?;
    }
    ensure!(
        verify_staged_database(&tx, "迁移数据库校验失败")? == count,
        "迁移过程中历史记录数量发生变化"
    );
    check_integrity(&tx)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;
