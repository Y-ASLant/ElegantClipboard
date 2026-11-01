use super::data::lock_instance;
use crate::instance;
use anyhow::{Context, Result};
use directories::BaseDirs;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub(super) struct DataDirectory {
    pub(super) path: PathBuf,
    pub(super) lock: File,
}

/// 显式 `--data-dir` 原样使用，否则取当前默认目录。不创建、不加锁。
pub(super) fn resolve(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_owned());
    }
    let base = BaseDirs::new().context("无法确定用户数据目录")?;
    #[cfg(target_os = "linux")]
    let name = "elegantclipboard";
    #[cfg(not(target_os = "linux"))]
    let name = "ElegantClipboard";
    Ok(base.data_local_dir().join(name))
}

pub(super) fn prepare(data_dir: Option<PathBuf>) -> Result<DataDirectory> {
    let path = resolve(data_dir.as_deref())?;
    let lock = lock_instance(&path)?;
    Ok(DataDirectory { path, lock })
}

pub(super) fn show_existing(data_dir: Option<PathBuf>) -> Result<bool> {
    instance::show_existing(&resolve(data_dir.as_deref())?)
}
