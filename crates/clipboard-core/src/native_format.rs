use crate::{History, blob_store::BlobStore};
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use std::path::PathBuf;

/// 单条额外原生格式的字节上限。
pub const MAX_NATIVE_FORMAT_BYTES: usize = 8 * 1024 * 1024;
/// 单次采集额外格式的总字节上限。
pub const MAX_NATIVE_FORMAT_TOTAL_BYTES: usize = 32 * 1024 * 1024;
/// 超过该大小的格式数据转外部 blob 文件。
pub const NATIVE_INLINE_MAX_BYTES: usize = 64 * 1024;

const MAX_FORMAT_NAME_CHARS: usize = 256;

/// 采集到的一条额外原生剪贴板格式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFormatCapture {
    pub name: String,
    pub data: Vec<u8>,
}

/// 额外格式未入库的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFormatSkip {
    pub name: String,
    pub reason: &'static str,
}

pub fn valid_format_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_FORMAT_NAME_CHARS
        && !name.chars().any(char::is_control)
}

/// 事务前暂存的 blob：Drop 删除 pending，提交成功后由调用方解除。
pub(crate) struct StagedBlob {
    hash: String,
    pending: PathBuf,
}

impl std::fmt::Debug for StagedBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedBlob")
            .field("hash", &self.hash)
            .finish()
    }
}

impl Drop for StagedBlob {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pending);
    }
}

impl StagedBlob {
    pub(crate) fn hash(&self) -> &str {
        &self.hash
    }
}

/// 事务内写入额外格式行；返回需要提交的 blob 暂存。
pub(crate) fn insert_rows_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    item_id: i64,
    rows: &[(String, Vec<u8>)],
    blob_root: &std::path::Path,
) -> Result<Vec<StagedBlob>> {
    let mut staged = Vec::new();
    {
        let mut insert = tx.prepare(
            "INSERT OR REPLACE INTO native_formats (item_id, format_name, data, blob_hash, byte_size)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (name, data) in rows {
            if data.len() > NATIVE_INLINE_MAX_BYTES {
                let (hash, pending) =
                    BlobStore::new(blob_root.to_path_buf()).write_pending(data)?;
                insert.execute(params![
                    item_id,
                    name,
                    Option::<Vec<u8>>::None,
                    hash,
                    data.len() as i64
                ])?;
                staged.push(StagedBlob { hash, pending });
            } else {
                insert.execute(params![
                    item_id,
                    name,
                    data,
                    Option::<String>::None,
                    data.len() as i64
                ])?;
            }
        }
    }
    Ok(staged)
}

/// 事务内按 item_ids 收集并删除额外格式行，返回释放的 blob 哈希。
pub(crate) fn detach_rows_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    item_ids: &[i64],
) -> Result<Vec<String>> {
    let mut hashes = Vec::new();
    if item_ids.is_empty() {
        return Ok(hashes);
    }
    let mut select = tx.prepare(
        "SELECT DISTINCT blob_hash FROM native_formats
         WHERE item_id = ?1 AND blob_hash IS NOT NULL",
    )?;
    let mut delete = tx.prepare("DELETE FROM native_formats WHERE item_id = ?1")?;
    for id in item_ids {
        let rows = select.query_map([id], |row| row.get::<_, String>(0))?;
        hashes.extend(rows.flatten());
        delete.execute([id])?;
    }
    Ok(hashes)
}

/// 事务内删除指定条目（按 id 列表）对应的别名行。
pub(crate) fn delete_aliases_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    content_hashes: &[String],
) -> Result<()> {
    let mut statement = tx.prepare("DELETE FROM content_hash_aliases WHERE canonical_hash = ?1")?;
    for hash in content_hashes {
        statement.execute([hash])?;
    }
    Ok(())
}

/// 清理指向已不存在条目的别名行（保留策略执行后兜底）。
pub(crate) fn purge_stale_aliases(tx: &rusqlite::Transaction<'_>) -> Result<usize> {
    let removed = tx.execute(
        "DELETE FROM content_hash_aliases
         WHERE canonical_hash NOT IN (SELECT content_hash FROM clipboard_items)",
        [],
    )?;
    Ok(removed)
}

impl History {
    /// 把采集到的额外格式挂到条目上；同名格式替换旧数据。
    /// 返回逐条跳过原因；条目不存在时整体报错。
    pub fn attach_native_formats(
        &self,
        item_id: i64,
        formats: Vec<NativeFormatCapture>,
    ) -> Result<Vec<NativeFormatSkip>> {
        let mut skips = Vec::new();
        let mut accepted: Vec<(String, Vec<u8>)> = Vec::new();
        let mut total = 0usize;
        for capture in formats {
            if !valid_format_name(&capture.name) {
                skips.push(NativeFormatSkip {
                    name: capture.name,
                    reason: "invalid-name",
                });
                continue;
            }
            if capture.data.is_empty() {
                skips.push(NativeFormatSkip {
                    name: capture.name,
                    reason: "empty",
                });
                continue;
            }
            if capture.data.len() > MAX_NATIVE_FORMAT_BYTES
                || total + capture.data.len() > MAX_NATIVE_FORMAT_TOTAL_BYTES
            {
                skips.push(NativeFormatSkip {
                    name: capture.name,
                    reason: "oversize",
                });
                continue;
            }
            total += capture.data.len();
            accepted.push((capture.name, capture.data));
        }
        if accepted.is_empty() {
            return Ok(skips);
        }
        let blob_root = self.blob_root()?;
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let exists: Option<i64> = tx
            .query_row(
                "SELECT id FROM clipboard_items WHERE id = ?1",
                [item_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            bail!("记录已不存在，无法保存原生格式");
        }
        // pending 文件在事务内创建；StagedBlob 的 Drop 保证失败路径不残留。
        let staged = insert_rows_in_transaction(&tx, item_id, &accepted, &blob_root)?;
        tx.commit()?;
        drop(connection);
        // 提交成功后正式落盘；此后 StagedBlob Drop 删 pending 是空操作。
        let store = BlobStore::new(blob_root);
        for blob in &staged {
            store.commit(blob.hash(), &blob.pending)?;
        }
        std::mem::forget(staged);
        Ok(skips)
    }

    /// 写回路径读取全部额外格式；blob 文件缺失的条目降级跳过而不失败。
    pub fn native_formats_for_copy(&self, item_id: i64) -> Result<Vec<(String, Vec<u8>)>> {
        let connection = self.db.read_connection();
        let connection = connection.lock();
        let mut statement = connection.prepare(
            "SELECT format_name, data, blob_hash FROM native_formats
             WHERE item_id = ?1 ORDER BY format_name",
        )?;
        let rows = statement.query_map([item_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let store = BlobStore::new(self.blob_root()?);
        let mut formats = Vec::new();
        for row in rows {
            let (name, inline, blob_hash) = row?;
            if let Some(data) = inline {
                formats.push((name, data));
            } else if let Some(hash) = blob_hash
                && let Ok(data) = store.read(&hash)
            {
                formats.push((name, data));
            }
        }
        Ok(formats)
    }

    pub(crate) fn blob_root(&self) -> Result<PathBuf> {
        Ok(self
            .db
            .path()
            .parent()
            .context("数据库路径缺少父目录")?
            .join("blobs"))
    }

    /// 删除条目对应的全部额外格式行，返回被释放的 blob 哈希（供提交后 GC）。
    pub(crate) fn detach_native_formats(&self, item_ids: &[i64]) -> Result<Vec<String>> {
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let hashes = detach_rows_in_transaction(&tx, item_ids)?;
        tx.commit()?;
        Ok(hashes)
    }

    /// 清空全部额外格式行（clear-all 用），返回释放的 blob 哈希。
    pub(crate) fn detach_all_native_formats(&self) -> Result<Vec<String>> {
        let connection = self.db.write_connection();
        let connection = connection.lock();
        let hashes: Vec<String> = {
            let mut statement = connection.prepare(
                "SELECT DISTINCT blob_hash FROM native_formats WHERE blob_hash IS NOT NULL",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.flatten().collect()
        };
        connection.execute("DELETE FROM native_formats", [])?;
        Ok(hashes)
    }

    /// 删除指向给定 content_hash 的别名行。
    pub(crate) fn delete_aliases(&self, content_hashes: &[String]) -> Result<()> {
        if content_hashes.is_empty() {
            return Ok(());
        }
        let connection = self.db.write_connection();
        let connection = connection.lock();
        let mut statement =
            connection.prepare("DELETE FROM content_hash_aliases WHERE canonical_hash = ?1")?;
        for hash in content_hashes {
            statement.execute([hash])?;
        }
        Ok(())
    }

    pub(crate) fn delete_all_aliases(&self) -> Result<()> {
        let connection = self.db.write_connection();
        let connection = connection.lock();
        connection.execute("DELETE FROM content_hash_aliases", [])?;
        Ok(())
    }

    /// 全库仍在引用的 blob 哈希集合（GC 的 used 集）。
    pub(crate) fn referenced_blob_hashes(&self) -> Result<std::collections::HashSet<String>> {
        let connection = self.db.read_connection();
        let connection = connection.lock();
        let mut statement = connection
            .prepare("SELECT DISTINCT blob_hash FROM native_formats WHERE blob_hash IS NOT NULL")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    }

    /// clear_history 路径：事务内按 clearable 条件收集并删除额外格式行与别名。
    pub(crate) fn clear_native_by_clearable_condition(
        &self,
        group_id: Option<i64>,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let (blobs, hashes) = clear_native_in_transaction(&tx, group_id)?;
        tx.commit()?;
        Ok((blobs, hashes))
    }
}

/// 事务内按 clearable 条件（未置顶未收藏，可按分组过滤）收集删除 blob 与别名。
pub(crate) fn clear_native_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    group_id: Option<i64>,
) -> Result<(Vec<String>, Vec<String>)> {
    let (group_cond, group_param) = match group_id {
        Some(id) => ("group_id = ?1", Some(id)),
        None => ("group_id IS NULL", None),
    };
    let condition = format!("is_pinned = 0 AND is_favorite = 0 AND {group_cond}");
    let blob_sql = format!(
        "SELECT DISTINCT nf.blob_hash FROM native_formats nf
         JOIN clipboard_items ci ON ci.id = nf.item_id
         WHERE nf.blob_hash IS NOT NULL AND {condition}"
    );
    let hash_sql = format!("SELECT DISTINCT content_hash FROM clipboard_items WHERE {condition}");
    let params_ref = rusqlite::params_from_iter(group_param.iter());
    let mut blobs = Vec::new();
    {
        let mut statement = tx.prepare(&blob_sql)?;
        let mut rows = statement.query(params_ref.clone())?;
        while let Some(row) = rows.next()? {
            blobs.push(row.get::<_, String>(0)?);
        }
    }
    let mut hashes = Vec::new();
    {
        let mut statement = tx.prepare(&hash_sql)?;
        let mut rows = statement.query(params_ref.clone())?;
        while let Some(row) = rows.next()? {
            hashes.push(row.get::<_, String>(0)?);
        }
    }
    let delete_sql = format!(
        "DELETE FROM native_formats WHERE item_id IN \
         (SELECT id FROM clipboard_items WHERE {condition})"
    );
    tx.execute(&delete_sql, params_ref)?;
    delete_aliases_in_transaction(tx, &hashes)?;
    Ok((blobs, hashes))
}

/// 保留策略路径：事务内按 id 列表收集并删除额外格式行与别名。
pub(crate) fn clear_native_by_ids(
    tx: &rusqlite::Transaction<'_>,
    ids: &[i64],
    content_hashes: &[String],
) -> Result<Vec<String>> {
    let blobs = detach_rows_in_transaction(tx, ids)?;
    delete_aliases_in_transaction(tx, content_hashes)?;
    purge_stale_aliases(tx)?;
    Ok(blobs)
}

#[cfg(test)]
mod tests;
