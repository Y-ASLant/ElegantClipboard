use crate::History;
use crate::backup_common::{
    MAX_ARCHIVE_BYTES, MAX_ASSET_BYTES, MAX_DATABASE_BYTES, MAX_ENTRIES,
    check_central_directory_names, ensure_destination_unused, install_staged_tree,
    remap_media_rows, remap_staged_payload, verify_staged_database,
};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, backup::Backup, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

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
    missing_staged: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub destination: PathBuf,
    pub total_items: i64,
    pub included_images: usize,
    pub missing_images: usize,
    pub included_icons: usize,
    pub missing_icons: usize,
    pub included_staged: usize,
    pub missing_staged: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub total_items: i64,
    pub restored_images: usize,
    pub missing_images: usize,
    pub restored_icons: usize,
    pub missing_icons: usize,
    pub restored_staged: usize,
    pub missing_staged: usize,
}

type AssetMap = HashMap<String, (String, PathBuf)>;

/// blake3 摘要的 hex 文本，用于把备份内的原始路径派生成安全的媒体文件名。
pub fn hash_token(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn register_asset(
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

fn rebase_archive_path(
    raw: &str,
    category: &str,
    extracted: &HashMap<String, HashSet<String>>,
    data_dir: &Path,
) -> Result<Option<String>> {
    let prefix = format!("{category}/");
    let Some(filename) = raw.strip_prefix(&prefix) else {
        return Ok(None);
    };
    if !extracted
        .get(category)
        .is_some_and(|names| names.contains(filename))
    {
        bail!("备份中缺少引用的{category}文件：{filename}");
    }
    Ok(Some(
        data_dir
            .join(category)
            .join(filename)
            .to_string_lossy()
            .into_owned(),
    ))
}

impl History {
    /// Export an online SQLite snapshot and its referenced assets as a portable ZIP.
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
        let media_rows: Vec<(i64, Option<String>, Option<String>)> = {
            let mut query = tx.prepare(
                "SELECT id, source_app_icon, file_payload FROM clipboard_items
                 WHERE source_app_icon IS NOT NULL OR file_payload IS NOT NULL",
            )?;
            query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut icons = AssetMap::new();
        let mut staged = AssetMap::new();
        let mut missing_icons = 0;
        let mut missing_staged = 0;
        {
            let mut update_icon =
                tx.prepare("UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2")?;
            let mut update_payload =
                tx.prepare("UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2")?;
            for (id, icon, payload) in media_rows {
                if let Some(raw_icon) = icon {
                    if let Some(name) = register_asset(&raw_icon, &mut icons, "bin") {
                        update_icon.execute(params![format!("icons/{name}"), id])?;
                    } else {
                        missing_icons += 1;
                    }
                }
                if let Some(raw_payload) = payload
                    && let Some(mapped) = remap_staged_payload(&raw_payload, |raw_path| {
                        if let Some(name) = register_asset(raw_path, &mut staged, "bin") {
                            Ok(Some(format!("staged/{name}")))
                        } else {
                            missing_staged += 1;
                            Ok(None)
                        }
                    })?
                {
                    update_payload.execute(params![mapped, id])?;
                }
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
            missing_staged,
        };
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
            zip.write_all(&serde_json::to_vec(&manifest)?)?;
            zip.start_file("clipboard.db", options)?;
            io::copy(&mut File::open(&snapshot_path)?, &mut zip)?;
            for (name, path) in images.values() {
                zip.start_file(format!("images/{name}"), options)?;
                io::copy(&mut File::open(path)?, &mut zip)
                    .with_context(|| format!("读取图片失败：{}", path.display()))?;
            }
            for (category, assets) in [("icons", &icons), ("staged", &staged)] {
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
        Ok(BackupReport {
            destination: destination.to_path_buf(),
            total_items,
            included_images: images.len(),
            missing_images,
            included_icons: icons.len(),
            missing_icons,
            included_staged: staged.len(),
            missing_staged,
        })
    }
}

/// Restore a GPUI ZIP backup into an unused data directory. The caller holds
/// the destination instance lock and must not have an open database there.
pub fn restore_backup(archive_path: &Path, data_dir: &Path) -> Result<RestoreReport> {
    ensure_destination_unused(data_dir, "恢复")?;
    let source = File::open(archive_path).context("无法打开备份文件")?;
    let mut raw_source = source.try_clone()?;
    let mut archive = ZipArchive::new(source).context("所选文件不是有效的 ZIP 备份")?;
    preflight_archive(
        &mut archive,
        &mut raw_source,
        MAX_ENTRIES,
        MAX_ARCHIVE_BYTES,
    )?;
    let manifest: Manifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .context("备份缺少格式说明")?;
        if entry.size() > 64 * 1024 {
            bail!("备份格式说明过大");
        }
        serde_json::from_reader(&mut entry).context("备份格式说明无效")?
    };
    if manifest.format != "elegantclipboard-gpui" || !matches!(manifest.version, 1 | FORMAT_VERSION)
    {
        bail!("不支持的备份格式或版本");
    }
    let stage = tempfile::Builder::new()
        .prefix(".clipboard-restore-")
        .tempdir_in(data_dir)
        .context("无法创建恢复暂存目录")?;
    let staged_db = stage.path().join("clipboard.db");
    {
        let mut entry = archive.by_name("clipboard.db").context("备份缺少数据库")?;
        if entry.size() > MAX_DATABASE_BYTES {
            bail!("备份数据库超过允许大小");
        }
        let copied = io::copy(&mut entry, &mut File::create(&staged_db)?)?;
        if copied != entry.size() {
            bail!("备份数据库长度不一致");
        }
    }
    let mut extracted: HashMap<String, HashSet<String>> = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        if name == "clipboard.db" || name == "manifest.json" {
            continue;
        }
        if matches!(name.as_str(), "images/" | "icons/" | "staged/") && entry.is_dir() {
            continue;
        }
        let Some((category, filename)) = name.split_once('/') else {
            bail!("备份包含未知文件：{name}");
        };
        if !matches!(category, "images" | "icons" | "staged")
            || (manifest.version == 1 && category != "images")
        {
            bail!("备份包含未知文件：{name}");
        }
        if filename.is_empty()
            || !filename
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            || entry.is_dir()
            || entry.size() > MAX_ASSET_BYTES
            || !extracted
                .entry(category.to_owned())
                .or_default()
                .insert(filename.to_owned())
        {
            bail!("备份媒体条目无效：{name}");
        }
        let media_path = stage.path().join(category).join(filename);
        fs::create_dir_all(media_path.parent().unwrap())?;
        let copied = io::copy(&mut entry, &mut File::create(media_path)?)?;
        if copied != entry.size() {
            bail!("备份媒体条目长度不一致：{name}");
        }
    }

    let mut restored_db = Connection::open(&staged_db)?;
    let tx = restored_db.transaction()?;
    tx.execute(crate::preferences::PRUNE_NON_GPUI_SETTINGS_SQL, [])?;
    remap_media_rows(&tx, |image, icon, payload| {
        let mapped_image = image
            .map(|raw| rebase_archive_path(raw, "images", &extracted, data_dir))
            .transpose()?
            .flatten();
        let mapped_icon = icon
            .map(|raw| rebase_archive_path(raw, "icons", &extracted, data_dir))
            .transpose()?
            .flatten();
        let mapped_payload = payload
            .map(|raw| {
                remap_staged_payload(raw, |path| {
                    rebase_archive_path(path, "staged", &extracted, data_dir)
                })
            })
            .transpose()?
            .flatten();
        Ok((mapped_image, mapped_icon, mapped_payload))
    })?;
    tx.commit()?;
    let total_items = verify_staged_database(&restored_db, "恢复数据库校验失败")?;
    if total_items != manifest.total_items {
        bail!("备份记录数量与格式说明不一致");
    }
    crate::import::checkpoint_staged_database(&restored_db)?;
    drop(restored_db);

    install_staged_tree(
        stage.path(),
        &staged_db,
        data_dir,
        "无法安装备份媒体文件",
        "无法安装备份数据库",
    )?;
    Ok(RestoreReport {
        total_items,
        restored_images: extracted.get("images").map_or(0, HashSet::len),
        missing_images: manifest.missing_images,
        restored_icons: extracted.get("icons").map_or(0, HashSet::len),
        missing_icons: manifest.missing_icons,
        restored_staged: extracted.get("staged").map_or(0, HashSet::len),
        missing_staged: manifest.missing_staged,
    })
}

fn preflight_archive(
    archive: &mut ZipArchive<File>,
    raw_source: &mut File,
    max_entries: usize,
    max_bytes: u64,
) -> Result<()> {
    check_central_directory_names(
        raw_source,
        archive.central_directory_start(),
        archive.len(),
        max_entries,
    )?;
    if archive.len() > max_entries {
        bail!("备份中的文件数量超过限制");
    }
    let mut names = HashSet::new();
    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        if !names.insert(name.clone()) {
            bail!("备份包含重复条目：{name}");
        }
        if matches!(name.as_str(), "manifest.json" | "clipboard.db") && entry.is_dir() {
            bail!("备份条目类型无效：{name}");
        }
        total_bytes = total_bytes
            .checked_add(entry.size())
            .filter(|size| *size <= max_bytes)
            .context("备份展开后超过允许大小")?;
    }
    if !names.contains("manifest.json") || !names.contains("clipboard.db") {
        bail!("备份缺少格式说明或数据库");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
