use super::DataSizeInfo;
use crate::instance;
use anyhow::{Context, Result, bail};
use clipboard_core::{
    backup::{RestoreReport, restore_backup},
    import::{ImportReport, import_legacy_database},
    legacy_backup::{LegacyBackupReport, import_legacy_backup},
};
use directories::ProjectDirs;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct InstanceBusy;

impl std::fmt::Display for InstanceBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("该数据目录已由另一个基础版实例使用")
    }
}

impl std::error::Error for InstanceBusy {}

pub(super) fn resolve_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf> {
    match data_dir {
        Some(path) => Ok(path),
        None => Ok(ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")
            .context("无法确定用户数据目录")?
            .data_local_dir()
            .to_owned()),
    }
}

pub(super) fn lock_instance(data_dir: &Path) -> Result<File> {
    std::fs::create_dir_all(data_dir).context("无法创建数据目录")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data_dir.join("instance.lock"))?;
    file.try_lock().map_err(|_| InstanceBusy)?;
    Ok(file)
}

pub fn show_existing_instance(data_dir: Option<PathBuf>) -> Result<bool> {
    instance::show_existing(&resolve_data_dir(data_dir)?)
}

pub fn import_legacy_data(
    source: &Path,
    data_dir: Option<PathBuf>,
) -> Result<(PathBuf, ImportReport)> {
    let data_dir = resolve_data_dir(data_dir)?;
    let _instance_lock = lock_instance(&data_dir)?;
    let destination = data_dir.join("clipboard.db");
    let report = import_legacy_database(source, &destination)?;
    Ok((destination, report))
}

pub fn restore_backup_data(source: &Path, data_dir: PathBuf) -> Result<(PathBuf, RestoreReport)> {
    let _instance_lock = lock_instance(&data_dir)?;
    let data_dir = data_dir.canonicalize().context("无法解析恢复目录")?;
    let report = restore_backup(source, &data_dir)?;
    Ok((data_dir.join("clipboard.db"), report))
}

pub fn import_legacy_backup_data(
    source: &Path,
    data_dir: PathBuf,
) -> Result<(PathBuf, LegacyBackupReport)> {
    let _instance_lock = lock_instance(&data_dir)?;
    let data_dir = data_dir.canonicalize().context("无法解析导入目录")?;
    let report = import_legacy_backup(source, &data_dir)?;
    Ok((data_dir.join("clipboard.db"), report))
}

fn file_size_if_present(path: &Path) -> Result<u64> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => bail!("{} 不是普通文件", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error).with_context(|| format!("无法读取 {}", path.display())),
    }
}

fn directory_size_and_count(root: &Path) -> Result<(u64, u64)> {
    if !root.exists() {
        return Ok((0, 0));
    }
    if !root.is_dir() {
        bail!("{} 不是目录", root.display());
    }
    let mut bytes = 0u64;
    let mut count = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .with_context(|| format!("无法读取目录 {}", directory.display()))?
        {
            let entry = entry.with_context(|| format!("无法读取目录项 {}", directory.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("无法读取文件类型 {}", entry.path().display()))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let length = entry
                    .metadata()
                    .with_context(|| format!("无法读取文件信息 {}", entry.path().display()))?
                    .len();
                bytes = bytes
                    .checked_add(length)
                    .context("数据目录大小超过可表示范围")?;
                count = count
                    .checked_add(1)
                    .context("数据目录文件数超过可表示范围")?;
            }
        }
    }
    Ok((bytes, count))
}

pub(super) fn data_size_info(
    data_dir: &Path,
    images_dir: &Path,
    staged_dir: &Path,
) -> Result<DataSizeInfo> {
    let database_bytes = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"]
        .into_iter()
        .try_fold(0u64, |total, name| {
            total
                .checked_add(file_size_if_present(&data_dir.join(name))?)
                .context("数据库大小超过可表示范围")
        })?;
    let (image_bytes, image_count) = directory_size_and_count(images_dir)?;
    let (staged_bytes, staged_count) = directory_size_and_count(staged_dir)?;
    let total_bytes = database_bytes
        .checked_add(image_bytes)
        .and_then(|total| total.checked_add(staged_bytes))
        .context("数据占用超过可表示范围")?;
    Ok(DataSizeInfo {
        database_bytes,
        image_bytes,
        image_count,
        staged_bytes,
        staged_count,
        total_bytes,
    })
}
