//! Shared plumbing for the GPUI backup restore and the legacy backup import:
//! size limits, staged-database verification, media remapping and the
//! install-with-rollback sequence. The two flows keep their own path
//! resolution strategies (strict vs. lenient) on top of these helpers.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction, params};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub(crate) const MAX_DATABASE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
pub(crate) const MAX_ASSET_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 100_000;

const MANAGED_ENTRY_NAMES: [&str; 4] = ["clipboard.db", "images", "icons", "staged"];

pub(crate) struct MediaRow {
    pub id: i64,
    pub image: Option<String>,
    pub icon: Option<String>,
    pub payload: Option<String>,
}

// zip 8.x keeps central-directory entries in an IndexMap and silently replaces
// earlier entries with the same raw name. Inspect the directory before trusting
// ZipArchive::len() or by_name() for restore decisions.
pub(crate) fn check_central_directory_names(
    file: &mut File,
    start: u64,
    parsed_count: usize,
    max_entries: usize,
) -> Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let mut names = HashSet::new();
    loop {
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature)?;
        if signature != *b"PK\x01\x02" {
            break;
        }
        let mut header = [0u8; 42];
        file.read_exact(&mut header)?;
        let name_len = u16::from_le_bytes([header[24], header[25]]) as usize;
        let extra_len = u16::from_le_bytes([header[26], header[27]]) as i64;
        let comment_len = u16::from_le_bytes([header[28], header[29]]) as i64;
        let mut name = vec![0; name_len];
        file.read_exact(&mut name)?;
        if !names.insert(name) {
            bail!("备份包含重复条目");
        }
        if names.len() > max_entries {
            bail!("备份中的文件数量超过限制");
        }
        file.seek(SeekFrom::Current(extra_len + comment_len))?;
    }
    if names.len() != parsed_count {
        bail!("备份目录条目数量不一致");
    }
    Ok(())
}

/// Rejects data directories that already hold a database or managed media.
/// `action` is the human-readable verb ("恢复" / "导入") used in the message.
pub(crate) fn ensure_destination_unused(data_dir: &Path, action: &str) -> Result<()> {
    if MANAGED_ENTRY_NAMES
        .into_iter()
        .any(|name| data_dir.join(name).exists())
    {
        bail!("目标数据目录已有数据库或媒体文件，{action}不会覆盖现有数据");
    }
    Ok(())
}

/// Rewrites media references for every history row. `resolve` returns the new
/// `(image_path, source_app_icon, file_payload)`; `None` components keep the
/// original value, and the UPDATE only runs when something actually changed.
pub(crate) fn remap_media_rows(
    tx: &Transaction<'_>,
    mut resolve: impl FnMut(
        Option<&str>,
        Option<&str>,
        Option<&str>,
    ) -> Result<(Option<String>, Option<String>, Option<String>)>,
) -> Result<()> {
    let mut query =
        tx.prepare("SELECT id, image_path, source_app_icon, file_payload FROM clipboard_items")?;
    let rows = query
        .query_map([], |row| {
            Ok(MediaRow {
                id: row.get(0)?,
                image: row.get(1)?,
                icon: row.get(2)?,
                payload: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(query);

    for MediaRow {
        id,
        image,
        icon,
        payload,
    } in rows
    {
        let (mapped_image, mapped_icon, mapped_payload) =
            resolve(image.as_deref(), icon.as_deref(), payload.as_deref())?;
        if mapped_image.is_some() || mapped_icon.is_some() || mapped_payload.is_some() {
            tx.execute(
                "UPDATE clipboard_items SET image_path = ?1, source_app_icon = ?2,
                 file_payload = ?3 WHERE id = ?4",
                params![
                    mapped_image.or(image),
                    mapped_icon.or(icon),
                    mapped_payload.or(payload),
                    id
                ],
            )?;
        }
    }
    Ok(())
}

/// Runs `PRAGMA quick_check` and returns the total history count.
/// `context` names the flow for the failure message.
pub(crate) fn verify_staged_database(
    connection: &Connection,
    context: &'static str,
) -> Result<i64> {
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("{context}：{integrity}");
    }
    Ok(connection.query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))?)
}

/// Moves the staged media categories and database from `stage_path` into
/// `data_dir`, rolling back installed media if any rename fails. The caller
/// must not have an open connection to `staged_db`.
pub(crate) fn install_staged_tree(
    stage_path: &Path,
    staged_db: &Path,
    data_dir: &Path,
    media_context: &'static str,
    db_context: &'static str,
) -> Result<()> {
    let mut installed = Vec::new();
    for category in ["images", "icons", "blobs"] {
        let source = stage_path.join(category);
        if source.exists() {
            let final_path = data_dir.join(category);
            if let Err(error) = std::fs::rename(&source, &final_path) {
                for (installed_path, original_path) in installed.into_iter().rev() {
                    let _ = std::fs::rename(installed_path, original_path);
                }
                return Err(error).context(media_context);
            }
            installed.push((final_path, source));
        }
    }
    if let Err(error) = std::fs::rename(staged_db, data_dir.join("clipboard.db")) {
        for (installed_path, original_path) in installed.into_iter().rev() {
            let _ = std::fs::rename(installed_path, original_path);
        }
        return Err(error).context(db_context);
    }
    Ok(())
}
