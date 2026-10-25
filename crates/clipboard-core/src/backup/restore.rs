use super::{FORMAT_VERSION, Manifest, RestoreReport};
use crate::backup_common::{
    MAX_ARCHIVE_BYTES, MAX_ASSET_BYTES, MAX_DATABASE_BYTES, MAX_ENTRIES,
    check_central_directory_names, ensure_destination_unused, install_staged_tree,
    remap_media_rows, remap_staged_payload, verify_staged_database,
};
use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io,
    path::Path,
};
use zip::ZipArchive;

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
        if matches!(name.as_str(), "images/" | "icons/" | "staged/" | "blobs/") && entry.is_dir() {
            continue;
        }
        let Some((category, filename)) = name.split_once('/') else {
            bail!("备份包含未知文件：{name}");
        };
        if !matches!(category, "images" | "icons" | "staged" | "blobs")
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
        restored_blobs: extracted.get("blobs").map_or(0, HashSet::len),
        missing_blobs: manifest.missing_blobs,
    })
}

pub(crate) fn preflight_archive(
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
