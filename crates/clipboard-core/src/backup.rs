use crate::History;
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

const FORMAT_VERSION: u32 = 1;
const MAX_DATABASE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    total_items: i64,
    missing_images: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    pub destination: PathBuf,
    pub total_items: i64,
    pub included_images: usize,
    pub missing_images: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub total_items: i64,
    pub restored_images: usize,
    pub missing_images: usize,
}

impl History {
    /// Export an online SQLite snapshot and its referenced images as a portable ZIP.
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

        // The GPUI backup contains only settings this application consumes.
        snapshot.execute(
            "DELETE FROM settings WHERE key NOT IN
             ('gpui_theme_mode', 'gpui_hotkey', 'gpui_capture_paused')",
            [],
        )?;
        let image_rows: Vec<(i64, String)> = {
            let mut query = snapshot.prepare(
                "SELECT id, image_path FROM clipboard_items WHERE image_path IS NOT NULL",
            )?;
            query
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?
        };
        let mut images: HashMap<String, (String, PathBuf)> = HashMap::new();
        let mut missing_images = 0;
        for (id, raw_path) in image_rows {
            let path = PathBuf::from(&raw_path);
            if !path.is_file() {
                missing_images += 1;
                continue;
            }
            let archive_name = if let Some((name, _)) = images.get(&raw_path) {
                name.clone()
            } else {
                let extension = path
                    .extension()
                    .and_then(|part| part.to_str())
                    .filter(|part| {
                        part.len() <= 8 && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    })
                    .unwrap_or("png");
                let name = format!(
                    "{}.{}",
                    blake3::hash(raw_path.as_bytes()).to_hex(),
                    extension
                );
                images.insert(raw_path.clone(), (name.clone(), path));
                name
            };
            snapshot.execute(
                "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
                params![format!("images/{archive_name}"), id],
            )?;
        }
        let integrity: String = snapshot.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            bail!("备份副本校验失败：{integrity}");
        }
        let total_items: i64 =
            snapshot.query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))?;
        drop(snapshot);

        let manifest = Manifest {
            format: "elegantclipboard-gpui".into(),
            version: FORMAT_VERSION,
            total_items,
            missing_images,
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
        })
    }
}

/// Restore a GPUI ZIP backup into an unused data directory. The caller holds
/// the destination instance lock and must not have an open database there.
pub fn restore_backup(archive_path: &Path, data_dir: &Path) -> Result<RestoreReport> {
    let destination = data_dir.join("clipboard.db");
    if destination.exists() || data_dir.join("images").exists() {
        bail!("目标数据目录已有数据库或图片，恢复不会覆盖现有数据");
    }
    let mut archive = ZipArchive::new(File::open(archive_path).context("无法打开备份文件")?)
        .context("所选文件不是有效的 ZIP 备份")?;
    let manifest: Manifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .context("备份缺少格式说明")?;
        if entry.size() > 64 * 1024 {
            bail!("备份格式说明过大");
        }
        serde_json::from_reader(&mut entry).context("备份格式说明无效")?
    };
    if manifest.format != "elegantclipboard-gpui" || manifest.version != FORMAT_VERSION {
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
        io::copy(&mut entry, &mut File::create(&staged_db)?)?;
    }
    let mut extracted = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if name == "clipboard.db" || name == "manifest.json" || name == "images/" {
            continue;
        }
        let Some(filename) = name.strip_prefix("images/") else {
            bail!("备份包含未知文件：{name}");
        };
        if filename.is_empty()
            || !filename
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            || entry.is_dir()
            || entry.size() > MAX_IMAGE_BYTES
            || !extracted.insert(filename.to_owned())
        {
            bail!("备份图片条目无效：{name}");
        }
        let image_path = stage.path().join("images").join(filename);
        fs::create_dir_all(image_path.parent().unwrap())?;
        io::copy(&mut entry, &mut File::create(image_path)?)?;
    }

    let mut restored_db = Connection::open(&staged_db)?;
    let tx = restored_db.transaction()?;
    let relative_paths: Vec<(i64, String)> = {
        let mut query = tx.prepare(
            "SELECT id, image_path FROM clipboard_items WHERE image_path LIKE 'images/%'",
        )?;
        query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    for (id, relative_path) in relative_paths {
        let filename = relative_path
            .strip_prefix("images/")
            .filter(|filename| extracted.contains(*filename))
            .context("备份中缺少引用的图片")?;
        let absolute_path = data_dir.join("images").join(filename);
        tx.execute(
            "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
            params![absolute_path.to_string_lossy().as_ref(), id],
        )?;
    }
    tx.commit()?;
    let integrity: String = restored_db.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("恢复数据库校验失败：{integrity}");
    }
    let total_items: i64 =
        restored_db.query_row("SELECT COUNT(*) FROM clipboard_items", [], |row| row.get(0))?;
    if total_items != manifest.total_items {
        bail!("备份记录数量与格式说明不一致");
    }
    drop(restored_db);

    let staged_images = stage.path().join("images");
    let final_images = data_dir.join("images");
    if staged_images.exists() {
        fs::rename(&staged_images, &final_images).context("无法安装备份图片")?;
    }
    if let Err(error) = fs::rename(&staged_db, &destination) {
        if final_images.exists() {
            let _ = fs::rename(&final_images, &staged_images);
        }
        return Err(error).context("无法安装备份数据库");
    }
    Ok(RestoreReport {
        total_items,
        restored_images: extracted.len(),
        missing_images: manifest.missing_images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        History, PreviewContent,
        preferences::{Preferences, ThemePreference},
    };

    #[test]
    fn live_backup_restores_text_groups_images_and_gpui_settings() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source");
        fs::create_dir_all(&source)?;
        let path = source.join("clipboard.db");
        let history = History::open(path)?;
        let id = history.capture("backup text")?.unwrap();
        history.toggle_favorite(id)?;
        let group = history.create_group("工作")?;
        history.move_to_group(id, None, Some(group.id))?;
        let png = b"\x89PNG\r\n\x1a\nsynthetic";
        let image = history.capture_image(png, 3, 2, &source.join("images"))?;
        Preferences::new(&history.db).set_theme(ThemePreference::Dark)?;
        history.db.write_connection().lock().execute(
            "INSERT INTO settings (key, value) VALUES ('secret_token', 'do-not-export')",
            [],
        )?;
        let backup = dir.path().join("history.zip");
        let exported = history.export_backup(&backup)?;
        assert_eq!(exported.total_items, 2);
        assert_eq!(exported.included_images, 1);
        assert_eq!(exported.missing_images, 0);
        assert!(history.export_backup(&backup).is_err());

        let target = dir.path().join("restored");
        fs::create_dir_all(&target)?;
        let report = restore_backup(&backup, &target)?;
        assert_eq!(report.total_items, 2);
        assert_eq!(report.restored_images, 1);
        let restored = History::open(target.join("clipboard.db"))?;
        assert_eq!(restored.groups()?[0].name, "工作");
        assert!(restored.item(id)?.is_favorite);
        assert_eq!(restored.text(id)?, "backup text");
        assert_eq!(
            Preferences::new(&restored.db).theme()?,
            ThemePreference::Dark
        );
        assert!(
            matches!(restored.preview_content(image)?, PreviewContent::Image(path) if path.starts_with(target.join("images")) && fs::read(&path)? == png)
        );
        assert_eq!(
            restored.db.read_connection().lock().query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
                [],
                |row| row.get::<_, i64>(0),
            )?,
            0
        );
        assert!(restore_backup(&backup, &target).is_err());
        Ok(())
    }

    #[test]
    fn invalid_backup_cannot_create_destination_database() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let invalid = dir.path().join("invalid.zip");
        fs::write(&invalid, b"not a zip")?;
        let target = dir.path().join("target");
        fs::create_dir_all(&target)?;
        assert!(restore_backup(&invalid, &target).is_err());
        assert!(!target.join("clipboard.db").exists());
        Ok(())
    }

    #[test]
    fn archive_path_traversal_is_rejected_before_installation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let source = History::open(dir.path().join("source.db"))?;
        source.capture("safe text")?;
        let valid = dir.path().join("valid.zip");
        source.export_backup(&valid)?;
        let malicious = dir.path().join("malicious.zip");
        let mut input = ZipArchive::new(File::open(valid)?)?;
        let mut output = ZipWriter::new(File::create(&malicious)?);
        for index in 0..input.len() {
            let mut entry = input.by_index(index)?;
            output.start_file(entry.name(), SimpleFileOptions::default())?;
            io::copy(&mut entry, &mut output)?;
        }
        output.start_file("images/../outside.txt", SimpleFileOptions::default())?;
        output.write_all(b"outside")?;
        output.finish()?;
        let target = dir.path().join("restored");
        fs::create_dir_all(&target)?;
        assert!(restore_backup(&malicious, &target).is_err());
        assert!(!target.join("clipboard.db").exists());
        assert!(!target.join("outside.txt").exists());
        Ok(())
    }
}
