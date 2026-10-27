use crate::History;
use crate::backup_common::verify_staged_database;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, backup::Backup, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

mod restore;

pub use restore::restore_backup;

const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    total_items: i64,
    missing_images: usize,
    #[serde(default)]
    missing_icons: usize,
    #[serde(default)]
    missing_blobs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub destination: PathBuf,
    pub total_items: i64,
    pub included_images: usize,
    pub missing_images: usize,
    pub included_icons: usize,
    pub missing_icons: usize,
    pub included_blobs: usize,
    pub missing_blobs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub total_items: i64,
    pub restored_images: usize,
    pub missing_images: usize,
    pub restored_icons: usize,
    pub missing_icons: usize,
    pub restored_blobs: usize,
    pub missing_blobs: usize,
}

pub(super) type AssetMap = HashMap<String, (String, PathBuf)>;

/// blake3 摘要的 hex 文本，用于把备份内的原始路径派生成安全的媒体文件名。
pub fn hash_token(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub(super) fn register_asset(
    raw_path: &str,
    assets: &mut AssetMap,
    fallback_extension: &str,
) -> Option<String> {
    let path = PathBuf::from(raw_path);
    if !path.is_file() {
        return None;
    }
    if let Some((name, _)) = assets.get(raw_path) {
        return Some(name.clone());
    }
    let extension = path
        .extension()
        .and_then(|part| part.to_str())
        .filter(|part| part.len() <= 8 && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .unwrap_or(fallback_extension);
    let name = format!("{}.{}", hash_token(raw_path.as_bytes()), extension);
    assets.insert(raw_path.to_owned(), (name.clone(), path));
    Some(name)
}

impl History {
    pub fn export_backup(&self, destination: &Path) -> Result<BackupReport> {
        if destination.exists() {
            bail!("备份文件已存在，请选择新文件名");
        }
        let parent = destination.parent().context("备份路径缺少父目录")?;
        if !parent.is_dir() {
            bail!("备份目录不存在：{}", parent.display());
        }
        let stage = tempfile::Builder::new()
            .prefix(".clipboard-backup-")
            .tempdir()
            .context("无法创建备份暂存目录")?;
        let snapshot_path = stage.path().join("clipboard.db");
        let mut snapshot = Connection::open(&snapshot_path)?;
        {
            let source = self.db.write_connection();
            let source = source.lock();
            Backup::new(&source, &mut snapshot)?
                .run_to_completion(64, Duration::from_millis(25), None)
                .context("数据库在线备份失败")?;
        }

        crate::file_copy_cleanup::clear_file_copy_references(&mut snapshot)?;
        // Keep snapshot settings and media rewrites in one transaction; the
        // source database has already been released by the online backup.
        let tx = snapshot.transaction()?;
        // The GPUI backup contains only settings this application consumes.
        tx.execute(crate::preferences::PRUNE_NON_GPUI_SETTINGS_SQL, [])?;
        let image_rows: Vec<(i64, String)> = {
            let mut query = tx.prepare(
                "SELECT id, image_path FROM clipboard_items WHERE image_path IS NOT NULL",
            )?;
            query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut images = AssetMap::new();
        let mut missing_images = 0;
        {
            let mut update =
                tx.prepare("UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2")?;
            for (id, raw_path) in image_rows {
                let Some(name) = register_asset(&raw_path, &mut images, "png") else {
                    missing_images += 1;
                    continue;
                };
                update.execute(params![format!("images/{name}"), id])?;
            }
        }
        let icon_rows: Vec<(i64, String)> = {
            let mut query = tx.prepare(
                "SELECT id, source_app_icon FROM clipboard_items WHERE source_app_icon IS NOT NULL",
            )?;
            query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut icons = AssetMap::new();
        let mut missing_icons = 0;
        {
            let mut update =
                tx.prepare("UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2")?;
            for (id, raw_icon) in icon_rows {
                if let Some(name) = register_asset(&raw_icon, &mut icons, "bin") {
                    update.execute(params![format!("icons/{name}"), id])?;
                } else {
                    missing_icons += 1;
                }
            }
        }
        // 额外格式 blob：文件名即内容哈希，恢复后无需重映射。
        let blob_hashes: Vec<String> = self
            .referenced_blob_hashes()?
            .into_iter()
            .collect::<Vec<_>>();
        let blob_root = self.blob_root()?;
        let mut blobs = AssetMap::new();
        let mut missing_blobs = 0;
        for hash in &blob_hashes {
            let path = blob_root.join(hash);
            if path.is_file() {
                blobs.insert(hash.clone(), (hash.clone(), path));
            } else {
                missing_blobs += 1;
            }
        }
        tx.commit()?;
        let total_items = verify_staged_database(&snapshot, "备份副本校验失败")?;
        crate::import::checkpoint_staged_database(&snapshot)?;
        drop(snapshot);

        let manifest = Manifest {
            format: "elegantclipboard-gpui".into(),
            version: FORMAT_VERSION,
            total_items,
            missing_images,
            missing_icons,
            missing_blobs,
        };
        write_backup_zip(
            &manifest,
            &snapshot_path,
            parent,
            [("images", &images), ("icons", &icons), ("blobs", &blobs)],
            destination,
        )?;
        Ok(BackupReport {
            destination: destination.to_path_buf(),
            total_items,
            included_images: images.len(),
            missing_images,
            included_icons: icons.len(),
            missing_icons,
            included_blobs: blobs.len(),
            missing_blobs,
        })
    }
}

fn write_backup_zip(
    manifest: &Manifest,
    snapshot_path: &Path,
    parent: &Path,
    categories: [(&str, &AssetMap); 3],
    destination: &Path,
) -> Result<()> {
    let mut output = tempfile::Builder::new()
        .prefix(".clipboard-export-")
        .tempfile_in(parent)
        .context("无法创建备份临时文件")?;
    {
        let mut zip = ZipWriter::new(output.as_file_mut());
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        zip.start_file("manifest.json", options)?;
        zip.write_all(&serde_json::to_vec(manifest)?)?;
        zip.start_file("clipboard.db", options)?;
        io::copy(&mut File::open(snapshot_path)?, &mut zip)?;
        for (category, assets) in categories {
            for (name, path) in assets.values() {
                zip.start_file(format!("{category}/{name}"), options)?;
                io::copy(&mut File::open(path)?, &mut zip)
                    .with_context(|| format!("读取媒体文件失败：{}", path.display()))?;
            }
        }
        zip.finish()?;
    }
    output.as_file_mut().sync_all()?;
    output
        .persist_noclobber(destination)
        .map_err(|error| error.error)
        .context("保存备份文件失败，目标可能已存在")?;
    Ok(())
}

#[cfg(test)]
mod tests;
