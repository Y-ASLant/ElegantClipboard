//! 清理已停用的普通文件副本，保留历史内容和元数据。
use crate::History;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use std::{
    fs::{self, Metadata},
    path::{Component, Path, PathBuf},
};

impl History {
    /// 先清除旧副本引用，再仅回收 `data_dir/staged`。
    /// 磁盘清理失败时保留未完成目录，供后续调用重试。
    pub fn remove_file_copies(&self, data_dir: &Path) -> Result<()> {
        let database_path = std::path::absolute(self.db.path())?;
        ensure!(
            fs::canonicalize(data_dir)?
                == fs::canonicalize(database_path.parent().context("数据库目录缺失")?)?,
            "清理目录与历史数据库目录不一致"
        );
        {
            let writer = self.db.write_connection();
            clear_file_copy_references(&mut writer.lock())?;
        }
        let Some(root) = checked_staged_directory(data_dir)? else {
            return Ok(());
        };
        // 标准库删除目录时不会跟随子级符号链接或 Windows junction。
        fs::remove_dir_all(&root)
            .with_context(|| format!("无法清理旧文件副本 {}，下次启动将重试", root.display()))
    }
}

/// 清理备份、导入数据库中的引用，不读取或删除源附件。
pub(crate) fn clear_file_copy_references(connection: &mut Connection) -> Result<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // 再次 UPDATE 恢复 updated_at 会重新触发时间戳更新。
    // 触发器定义和行修改均受事务保护，失败时一同回滚。
    let trigger: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger'
             AND name = 'clipboard_items_update_timestamp'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    tx.execute_batch("DROP TRIGGER IF EXISTS clipboard_items_update_timestamp;")?;
    let has_payload: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('clipboard_items') WHERE name='file_payload')",
        [],
        |row| row.get(0),
    )?;
    let payloads = if has_payload {
        let mut query = tx.prepare(
            "SELECT id, file_payload FROM clipboard_items
             WHERE file_payload IS NOT NULL ORDER BY id",
        )?;
        query
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    for (id, payload) in payloads {
        if let Some(cleaned) = without_staged(&payload)
            .with_context(|| format!("清理历史记录 {id} 的旧文件副本引用失败"))?
        {
            tx.execute(
                "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
                params![cleaned, id],
            )?;
        }
    }
    let has_settings: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
        [],
        |row| row.get(0),
    )?;
    if has_settings {
        tx.execute("DELETE FROM settings WHERE key = 'gpui_stage_files'", [])?;
    }
    if let Some(sql) = trigger {
        tx.execute_batch(&sql)?;
    }
    tx.commit()?;
    Ok(())
}

// 未知 legacy 字段逐字节保留，避免重写大数字、精确小数或重复键。
fn without_staged(raw: &str) -> Result<Option<String>> {
    let mut validator = serde_json::Deserializer::from_str(raw);
    serde::de::IgnoredAny::deserialize(&mut validator).context("文件载荷 JSON 无效")?;
    validator.end()?;
    let mut position = 0;
    whitespace(raw, &mut position);
    if raw.as_bytes()[position] != b'{' {
        return Ok(None);
    }
    position += 1;
    let mut kept = Vec::new();
    let mut removed = false;
    loop {
        whitespace(raw, &mut position);
        if raw.as_bytes()[position] == b'}' {
            break;
        }
        let start = position;
        let mut key = serde_json::Deserializer::from_str(&raw[position..]).into_iter::<String>();
        let key_name = key.next().context("文件载荷字段缺失")??;
        position += key.byte_offset();
        whitespace(raw, &mut position);
        position += 1; // 冒号已通过前面的完整 JSON 语法校验。
        let mut value = serde_json::Deserializer::from_str(&raw[position..])
            .into_iter::<serde::de::IgnoredAny>();
        value.next().context("文件载荷字段值缺失")??;
        position += value.byte_offset();
        if key_name == "staged" {
            removed = true;
        } else {
            kept.push(&raw[start..position]);
        }
        whitespace(raw, &mut position);
        if raw.as_bytes()[position] == b',' {
            position += 1;
        } else {
            break;
        }
    }
    Ok(removed.then(|| format!("{{{}}}", kept.join(","))))
}

fn whitespace(raw: &str, position: &mut usize) {
    while raw
        .as_bytes()
        .get(*position)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *position += 1;
    }
}

pub(crate) fn is_reparse(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(crate) fn checked_staged_directory(data_dir: &Path) -> Result<Option<PathBuf>> {
    let data_dir = std::path::absolute(data_dir)?;
    ensure!(
        !data_dir
            .components()
            .any(|part| matches!(part, Component::ParentDir)),
        "数据目录含父级路径，未清理旧文件副本"
    );
    let root = data_dir.join("staged");
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("无法检查旧文件副本目录"),
    };
    for ancestor in data_dir.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)
            .with_context(|| format!("无法检查数据目录 {}", ancestor.display()))?;
        ensure!(
            metadata.is_dir() && !is_reparse(&metadata),
            "数据目录含符号链接或重解析路径，未清理旧文件副本：{}",
            ancestor.display()
        );
    }
    ensure!(
        metadata.is_dir() && !is_reparse(&metadata),
        "旧文件副本目录不是普通目录，未跟随符号链接或重解析路径：{}",
        root.display()
    );
    let root = fs::canonicalize(root)?;
    ensure!(
        root.parent() == Some(fs::canonicalize(data_dir)?.as_path()),
        "旧文件副本目录超出数据目录"
    );
    Ok(Some(root))
}

#[cfg(test)]
mod tests;
