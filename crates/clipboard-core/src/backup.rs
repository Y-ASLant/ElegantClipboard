use crate::History;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, backup::Backup, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const FORMAT_VERSION: u32 = 2;
const MAX_DATABASE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 1024 * 1024 * 1024;

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

struct BackupMediaRow {
    id: i64,
    image: Option<String>,
    icon: Option<String>,
    payload: Option<String>,
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
    let name = format!(
        "{}.{}",
        blake3::hash(raw_path.as_bytes()).to_hex(),
        extension
    );
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
        let mut images = AssetMap::new();
        let mut missing_images = 0;
        for (id, raw_path) in image_rows {
            let Some(name) = register_asset(&raw_path, &mut images, "png") else {
                missing_images += 1;
                continue;
            };
            snapshot.execute(
                "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
                params![format!("images/{name}"), id],
            )?;
        }
        let media_rows: Vec<(i64, Option<String>, Option<String>)> = {
            let mut query = snapshot.prepare(
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
        for (id, icon, payload) in media_rows {
            if let Some(raw_icon) = icon {
                if let Some(name) = register_asset(&raw_icon, &mut icons, "bin") {
                    snapshot.execute(
                        "UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2",
                        params![format!("icons/{name}"), id],
                    )?;
                } else {
                    missing_icons += 1;
                }
            }
            let Some(raw_payload) = payload else {
                continue;
            };
            let Ok(mut value) = serde_json::from_str::<Value>(&raw_payload) else {
                continue;
            };
            let Some(entries) = value.get_mut("staged").and_then(Value::as_array_mut) else {
                continue;
            };
            let mut changed = false;
            for entry in entries {
                let Some(raw_path) = entry.get("staged").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(name) = register_asset(raw_path, &mut staged, "bin") {
                    entry["staged"] = Value::String(format!("staged/{name}"));
                    changed = true;
                } else {
                    missing_staged += 1;
                }
            }
            if changed {
                snapshot.execute(
                    "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
                    params![value.to_string(), id],
                )?;
            }
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
    let destination = data_dir.join("clipboard.db");
    if ["clipboard.db", "images", "icons", "staged"]
        .into_iter()
        .any(|name| data_dir.join(name).exists())
    {
        bail!("目标数据目录已有数据库或媒体文件，恢复不会覆盖现有数据");
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
        io::copy(&mut entry, &mut File::create(&staged_db)?)?;
    }
    let mut extracted: HashMap<String, HashSet<String>> = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
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
        io::copy(&mut entry, &mut File::create(media_path)?)?;
    }

    let mut restored_db = Connection::open(&staged_db)?;
    let tx = restored_db.transaction()?;
    let media_rows: Vec<BackupMediaRow> = {
        let mut query = tx
            .prepare("SELECT id, image_path, source_app_icon, file_payload FROM clipboard_items")?;
        query
            .query_map([], |row| {
                Ok(BackupMediaRow {
                    id: row.get(0)?,
                    image: row.get(1)?,
                    icon: row.get(2)?,
                    payload: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?
    };
    for BackupMediaRow {
        id,
        image,
        icon,
        payload,
    } in media_rows
    {
        let mapped_image = image
            .as_deref()
            .map(|raw| rebase_archive_path(raw, "images", &extracted, data_dir))
            .transpose()?
            .flatten();
        let mapped_icon = icon
            .as_deref()
            .map(|raw| rebase_archive_path(raw, "icons", &extracted, data_dir))
            .transpose()?
            .flatten();
        let mut mapped_payload = None;
        if let Some(raw_payload) = payload.as_deref()
            && let Ok(mut value) = serde_json::from_str::<Value>(raw_payload)
            && let Some(entries) = value.get_mut("staged").and_then(Value::as_array_mut)
        {
            let mut changed = false;
            for entry in entries {
                let Some(raw_path) = entry.get("staged").and_then(Value::as_str) else {
                    continue;
                };
                if let Some(path) = rebase_archive_path(raw_path, "staged", &extracted, data_dir)? {
                    entry["staged"] = Value::String(path);
                    changed = true;
                }
            }
            if changed {
                mapped_payload = Some(value.to_string());
            }
        }
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

    let mut installed = Vec::new();
    for category in ["images", "icons", "staged"] {
        let source = stage.path().join(category);
        if source.exists() {
            let final_path = data_dir.join(category);
            if let Err(error) = fs::rename(&source, &final_path) {
                for (installed_path, original_path) in installed.into_iter().rev() {
                    let _ = fs::rename(installed_path, original_path);
                }
                return Err(error).context("无法安装备份媒体文件");
            }
            installed.push((final_path, source));
        }
    }
    if let Err(error) = fs::rename(&staged_db, &destination) {
        for (installed_path, original_path) in installed.into_iter().rev() {
            let _ = fs::rename(installed_path, original_path);
        }
        return Err(error).context("无法安装备份数据库");
    }
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
        let icon = source.join("icons/app.ico");
        fs::create_dir_all(icon.parent().unwrap())?;
        fs::write(&icon, b"icon bytes")?;
        let staged = source.join("staged/draft.txt");
        fs::create_dir_all(staged.parent().unwrap())?;
        fs::write(&staged, b"draft bytes")?;
        let original_file = "Z:\\ElegantClipboard-QA-missing\\draft.txt".to_owned();
        let file =
            history.capture_files(std::slice::from_ref(&original_file), &source.join("images"))?;
        let payload = serde_json::json!({
            "staged": [{"original": original_file, "staged": staged.to_string_lossy()}]
        });
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2",
            params![icon.to_string_lossy().as_ref(), id],
        )?;
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            params![payload.to_string(), file],
        )?;
        Preferences::new(&history.db).set_theme(ThemePreference::Dark)?;
        history.db.write_connection().lock().execute(
            "INSERT INTO settings (key, value) VALUES ('secret_token', 'do-not-export')",
            [],
        )?;
        let backup = dir.path().join("history.zip");
        let exported = history.export_backup(&backup)?;
        assert_eq!(exported.total_items, 3);
        assert_eq!(exported.included_images, 1);
        assert_eq!(exported.missing_images, 0);
        assert_eq!((exported.included_icons, exported.included_staged), (1, 1));
        assert!(history.export_backup(&backup).is_err());

        let target = dir.path().join("restored");
        fs::create_dir_all(&target)?;
        let report = restore_backup(&backup, &target)?;
        assert_eq!(report.total_items, 3);
        assert_eq!(report.restored_images, 1);
        assert_eq!((report.restored_icons, report.restored_staged), (1, 1));
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
        let icon = PathBuf::from(restored.item(id)?.source_app_icon.unwrap());
        assert!(icon.starts_with(target.join("icons")));
        assert_eq!(fs::read(icon)?, b"icon bytes");
        let copied = restored.files_for_copy(file, &target.join("staged"))?;
        assert_eq!(copied.len(), 1);
        assert_eq!(fs::read(&copied[0])?, b"draft bytes");
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
    fn restores_previous_gpui_v1_backup() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("source.db"))?;
        let id = history.capture("v1 text")?.unwrap();
        let current = dir.path().join("current.zip");
        history.export_backup(&current)?;
        let mut input = ZipArchive::new(File::open(current)?)?;
        let previous = dir.path().join("previous.zip");
        let mut output = ZipWriter::new(File::create(&previous)?);
        output.start_file("manifest.json", SimpleFileOptions::default())?;
        output.write_all(
            br#"{"format":"elegantclipboard-gpui","version":1,"total_items":1,"missing_images":0}"#,
        )?;
        output.start_file("clipboard.db", SimpleFileOptions::default())?;
        io::copy(&mut input.by_name("clipboard.db")?, &mut output)?;
        output.finish()?;
        let target = dir.path().join("restored");
        fs::create_dir_all(&target)?;
        let report = restore_backup(&previous, &target)?;
        assert_eq!(report.total_items, 1);
        assert_eq!((report.restored_icons, report.restored_staged), (0, 0));
        assert_eq!(
            History::open(target.join("clipboard.db"))?.text(id)?,
            "v1 text"
        );
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
