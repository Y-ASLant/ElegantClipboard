//! Import the ZIP layout written by the Tauri application into an unused GPUI directory.

use crate::import::{ImportReport, import_legacy_database};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, File},
    io,
    path::Path,
};
use zip::ZipArchive;

const MAX_DATABASE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

#[derive(Debug, PartialEq, Eq)]
pub struct LegacyBackupReport {
    pub items: ImportReport,
    pub images: usize,
    pub icons: usize,
    pub staged_files: usize,
    pub unbundled_images: usize,
}

#[derive(Default)]
struct Assets {
    images: HashMap<String, String>,
    icons: HashMap<String, String>,
    staged: HashMap<String, String>,
}

struct MediaRow {
    id: i64,
    image: Option<String>,
    icon: Option<String>,
    payload: Option<String>,
}

impl Assets {
    fn map_mut(&mut self, category: &str) -> &mut HashMap<String, String> {
        match category {
            "images" => &mut self.images,
            "icons" => &mut self.icons,
            "staged" => &mut self.staged,
            _ => unreachable!(),
        }
    }
}

pub fn import_legacy_backup(archive_path: &Path, data_dir: &Path) -> Result<LegacyBackupReport> {
    let data_dir = fs::canonicalize(data_dir).context("无法解析目标数据目录")?;
    if ["clipboard.db", "images", "icons", "staged"]
        .into_iter()
        .any(|name| data_dir.join(name).exists())
    {
        bail!("目标数据目录已有数据库或媒体文件，导入不会覆盖现有数据");
    }
    let mut archive = ZipArchive::new(File::open(archive_path).context("无法打开旧版备份")?)
        .context("所选文件不是有效的 ZIP 备份")?;
    if archive.len() > MAX_ENTRIES {
        bail!("备份中的文件数量超过限制");
    }
    let stage = tempfile::Builder::new()
        .prefix(".clipboard-legacy-restore-")
        .tempdir_in(&data_dir)?;
    let raw_db = stage.path().join("legacy.db");
    let mut seen_db = false;
    let mut assets = Assets::default();
    let mut total_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        if name == "manifest.json" {
            bail!("这是 GPUI 格式备份，请使用 --import-backup");
        }
        if matches!(name.as_str(), "images/" | "icons/" | "staged/") && entry.is_dir() {
            continue;
        }
        let limit = if name == "clipboard.db" {
            MAX_DATABASE_BYTES
        } else {
            MAX_ASSET_BYTES
        };
        total_bytes = total_bytes
            .checked_add(entry.size())
            .filter(|sum| *sum <= MAX_ARCHIVE_BYTES)
            .context("备份展开后超过允许大小")?;
        if entry.size() > limit || entry.is_dir() {
            bail!("备份条目过大或类型无效：{name}");
        }
        let destination = if name == "clipboard.db" {
            if seen_db {
                bail!("备份包含重复的数据库文件");
            }
            seen_db = true;
            raw_db.clone()
        } else {
            let (category, filename) = checked_asset_name(&name)?;
            let extension = filename
                .rsplit_once('.')
                .map(|(_, suffix)| suffix)
                .filter(|suffix| {
                    !suffix.is_empty()
                        && suffix.len() <= 8
                        && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
                .unwrap_or("bin");
            let safe_name = format!("{}.{}", blake3::hash(name.as_bytes()).to_hex(), extension);
            if assets
                .map_mut(category)
                .insert(filename.to_owned(), safe_name.clone())
                .is_some()
            {
                bail!("备份包含重复的媒体文件：{name}");
            }
            let path = stage.path().join(category).join(safe_name);
            fs::create_dir_all(path.parent().unwrap())?;
            path
        };
        let copied = io::copy(&mut entry, &mut File::create(&destination)?)?;
        if copied != entry.size() {
            bail!("备份条目长度不一致：{name}");
        }
    }
    if !seen_db {
        bail!("旧版备份缺少 clipboard.db");
    }

    let staged_db = stage.path().join("clipboard.db");
    let items = import_legacy_database(&raw_db, &staged_db)?;
    let mut connection = Connection::open(&staged_db)?;
    let mut unbundled_images = 0;
    {
        let tx = connection.transaction()?;
        let rows: Vec<MediaRow> = {
            let mut query = tx.prepare(
                "SELECT id, image_path, source_app_icon, file_payload FROM clipboard_items",
            )?;
            query
                .query_map([], |row| {
                    Ok(MediaRow {
                        id: row.get(0)?,
                        image: row.get(1)?,
                        icon: row.get(2)?,
                        payload: row.get(3)?,
                    })
                })?
                .collect::<Result<_, _>>()?
        };
        for MediaRow {
            id,
            image,
            icon,
            payload,
        } in rows
        {
            let mapped_image = image
                .as_deref()
                .and_then(|raw| rebase_path(raw, "images", &assets.images, &data_dir));
            if image.is_some() && mapped_image.is_none() {
                unbundled_images += 1;
            }
            let mapped_icon = icon
                .as_deref()
                .and_then(|raw| rebase_path(raw, "icons", &assets.icons, &data_dir));
            let mapped_payload = payload
                .as_deref()
                .and_then(|raw| rebase_staged_payload(raw, &assets.staged, &data_dir));
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
    }
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("旧版备份导入校验失败：{integrity}");
    }
    crate::import::checkpoint_staged_database(&connection)?;
    drop(connection);

    let mut installed = Vec::new();
    for category in ["images", "icons", "staged"] {
        let staged = stage.path().join(category);
        if staged.exists() {
            let final_path = data_dir.join(category);
            if let Err(error) = fs::rename(&staged, &final_path) {
                for (installed_path, original_path) in installed.into_iter().rev() {
                    let _ = fs::rename(installed_path, original_path);
                }
                return Err(error).context("无法安装旧版媒体文件");
            }
            installed.push((final_path, staged));
        }
    }
    if let Err(error) = fs::rename(&staged_db, data_dir.join("clipboard.db")) {
        for (installed_path, original_path) in installed.into_iter().rev() {
            let _ = fs::rename(installed_path, original_path);
        }
        return Err(error).context("无法安装旧版数据库");
    }
    Ok(LegacyBackupReport {
        items,
        images: assets.images.len(),
        icons: assets.icons.len(),
        staged_files: assets.staged.len(),
        unbundled_images,
    })
}

fn checked_asset_name(name: &str) -> Result<(&str, &str)> {
    let (category, filename) = name.split_once('/').context("备份包含未知文件")?;
    if !matches!(category, "images" | "icons" | "staged")
        || filename.is_empty()
        || filename == "."
        || filename == ".."
        || filename.len() > 255
        || filename.ends_with([' ', '.'])
        || filename.chars().any(|ch| {
            ch.is_control() || matches!(ch, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        bail!("备份中的媒体路径无效：{name}");
    }
    Ok((category, filename))
}

fn rebase_path(
    raw: &str,
    category: &str,
    names: &HashMap<String, String>,
    data_dir: &Path,
) -> Option<String> {
    let mut parts = raw.rsplit(['/', '\\']);
    let filename = parts.next()?;
    let parent = parts.next()?;
    if !parent.eq_ignore_ascii_case(category) {
        return None;
    }
    let safe_name = names.get(filename)?;
    Some(
        data_dir
            .join(category)
            .join(safe_name)
            .to_string_lossy()
            .into_owned(),
    )
}

fn rebase_staged_payload(
    raw: &str,
    names: &HashMap<String, String>,
    data_dir: &Path,
) -> Option<String> {
    let mut value: Value = serde_json::from_str(raw).ok()?;
    let staged = value.get_mut("staged")?.as_array_mut()?;
    let mut changed = false;
    for item in staged {
        let Some(path) = item.get("staged").and_then(Value::as_str) else {
            continue;
        };
        if let Some(new_path) = rebase_path(path, "staged", names, data_dir) {
            item["staged"] = Value::String(new_path);
            changed = true;
        }
    }
    changed.then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{History, PreviewContent};
    use rusqlite::backup::Backup;
    use std::{io::Write, path::PathBuf, time::Duration};
    use zip::{ZipWriter, write::SimpleFileOptions};

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nsynthetic";

    #[test]
    fn old_zip_restores_media_metadata_and_groups_without_settings_secrets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        fs::create_dir_all(&source)?;
        let history = History::open(source.join("clipboard.db"))?;
        let group = history.create_group("旧分组")?;
        let text = history.capture("旧版收藏")?.unwrap();
        history.move_to_group(text, None, Some(group.id))?;
        history.toggle_favorite(text)?;
        let image = history.capture_image(PNG, 3, 2, &source.join("images"))?;
        let image_path = PathBuf::from(history.item(image)?.image_path.unwrap());
        let icon_path = source.join("icons").join("应用.ico");
        fs::create_dir_all(icon_path.parent().unwrap())?;
        fs::write(&icon_path, b"icon bytes")?;
        let staged_path = source.join("staged").join("草稿.txt");
        fs::create_dir_all(staged_path.parent().unwrap())?;
        fs::write(&staged_path, b"draft bytes")?;
        let original_file = "Z:\\ElegantClipboard-QA-missing\\draft.txt".to_owned();
        let file =
            history.capture_files(std::slice::from_ref(&original_file), &source.join("images"))?;
        let payload = serde_json::json!({
            "staged": [{
                "original": original_file,
                "staged": staged_path.to_string_lossy(),
                "size": 11
            }]
        });
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2",
            params![icon_path.to_string_lossy().as_ref(), text],
        )?;
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            params![payload.to_string(), file],
        )?;
        history.db.write_connection().lock().execute(
            "INSERT INTO settings (key, value) VALUES ('secret_token', 'keep-out')",
            [],
        )?;
        let archive_path = directory.path().join("old.zip");
        let snapshot_path = directory.path().join("snapshot.db");
        {
            let mut snapshot = Connection::open(&snapshot_path)?;
            let connection = history.db.write_connection();
            let connection = connection.lock();
            Backup::new(&connection, &mut snapshot)?.run_to_completion(
                64,
                Duration::from_millis(1),
                None,
            )?;
        }
        let mut zip = ZipWriter::new(File::create(&archive_path)?);
        for (entry, source) in [
            ("clipboard.db".to_owned(), snapshot_path),
            (
                format!(
                    "images/{}",
                    image_path.file_name().unwrap().to_string_lossy()
                ),
                image_path,
            ),
            ("icons/应用.ico".to_owned(), icon_path),
            ("staged/草稿.txt".to_owned(), staged_path),
        ] {
            zip.start_file(entry, SimpleFileOptions::default())?;
            io::copy(&mut File::open(source)?, &mut zip)?;
        }
        zip.finish()?;

        let target = directory.path().join("restored");
        fs::create_dir_all(&target)?;
        let report = import_legacy_backup(&archive_path, &target)?;
        assert_eq!(report.items.total_items, 3);
        assert_eq!(
            (report.images, report.icons, report.staged_files),
            (1, 1, 1)
        );
        assert_eq!(report.unbundled_images, 0);
        let restored = History::open(target.join("clipboard.db"))?;
        assert_eq!(restored.groups()?[0].name, "旧分组");
        assert!(restored.item(text)?.is_favorite);
        assert_eq!(restored.text(text)?, "旧版收藏");
        assert!(
            matches!(restored.preview_content(image)?, PreviewContent::Image(path) if path.starts_with(fs::canonicalize(target.join("images"))?) && fs::read(&path)? == PNG)
        );
        let restored_text = restored.item(text)?;
        let icon = PathBuf::from(restored_text.source_app_icon.unwrap());
        assert!(icon.starts_with(fs::canonicalize(target.join("icons"))?));
        assert_eq!(fs::read(icon)?, b"icon bytes");
        let payload: Value = serde_json::from_str(&restored.item(file)?.file_payload.unwrap())?;
        let staged = PathBuf::from(payload["staged"][0]["staged"].as_str().unwrap());
        assert!(staged.starts_with(fs::canonicalize(target.join("staged"))?));
        assert_eq!(fs::read(staged)?, b"draft bytes");
        assert_eq!(restored.files(file)?, vec![original_file]);
        let copied = restored.files_for_copy(file, &target.join("staged"))?;
        assert_eq!(copied.len(), 1);
        assert_eq!(fs::read(&copied[0])?, b"draft bytes");
        assert_eq!(
            restored.db.read_connection().lock().query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            0
        );
        assert!(import_legacy_backup(&archive_path, &target).is_err());
        Ok(())
    }

    #[test]
    fn old_zip_rejects_traversal_without_creating_database() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let archive_path = directory.path().join("unsafe.zip");
        let mut zip = ZipWriter::new(File::create(&archive_path)?);
        zip.start_file("images/../outside.png", SimpleFileOptions::default())?;
        zip.write_all(PNG)?;
        zip.finish()?;
        let target = directory.path().join("restored");
        fs::create_dir_all(&target)?;
        assert!(import_legacy_backup(&archive_path, &target).is_err());
        assert!(!target.join("clipboard.db").exists());
        assert!(!target.join("outside.png").exists());
        Ok(())
    }
}
