use super::data::lock_instance;
use crate::instance;
use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

mod migration;

pub(super) struct DataDirectory {
    pub(super) path: PathBuf,
    pub(super) lock: File,
    pub(super) warning: Option<String>,
}

struct DefaultPaths {
    current: PathBuf,
    legacy: PathBuf,
}

impl DefaultPaths {
    fn system() -> Result<Self> {
        let base = BaseDirs::new().context("无法确定用户数据目录")?;
        let legacy = ProjectDirs::from("com", "ASLant", "ElegantClipboard-GPUI")
            .context("无法确定旧版数据目录")?;
        #[cfg(target_os = "linux")]
        let name = "elegantclipboard";
        #[cfg(not(target_os = "linux"))]
        let name = "ElegantClipboard";
        Ok(Self {
            current: base.data_local_dir().join(name),
            legacy: legacy.data_local_dir().to_owned(),
        })
    }

    fn is_default(&self, explicit: &Path) -> bool {
        same_path(explicit, &self.current)
            || same_path(explicit, &self.legacy)
            || std::fs::canonicalize(explicit).ok().is_some_and(|path| {
                same_path(&path, &self.current)
                    || same_path(&path, &self.legacy)
                    || self
                        .current
                        .canonicalize()
                        .is_ok_and(|current| path == current)
                    || self
                        .legacy
                        .canonicalize()
                        .is_ok_and(|legacy| path == legacy)
            })
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let absolute = |path: &Path| -> Option<PathBuf> {
        if path.is_absolute() {
            Some(path.to_owned())
        } else {
            Some(std::env::current_dir().ok()?.join(path))
        }
    };
    let (Some(left), Some(right)) = (absolute(left), absolute(right)) else {
        return false;
    };
    #[cfg(windows)]
    {
        let normalize = |path: &Path| {
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_lowercase()
        };
        normalize(&left) == normalize(&right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

pub(super) fn prepare(data_dir: Option<PathBuf>) -> Result<DataDirectory> {
    let paths = DefaultPaths::system();
    prepare_with_paths(data_dir, paths, crate::autostart::migrate_data_dir)
}

fn prepare_with_paths(
    data_dir: Option<PathBuf>,
    paths: Result<DefaultPaths>,
    migrate_autostart: impl Fn(&Path, &Path) -> Result<()>,
) -> Result<DataDirectory> {
    if let Some(path) = data_dir
        && !paths.as_ref().is_ok_and(|paths| paths.is_default(&path))
    {
        let lock = lock_instance(&path)?;
        return Ok(DataDirectory {
            path,
            lock,
            warning: None,
        });
    }
    migration::prepare_default(&paths?, migrate_autostart)
}

pub(super) fn show_existing(data_dir: Option<PathBuf>) -> Result<bool> {
    let paths = DefaultPaths::system();
    if let Some(path) = data_dir
        && !paths.as_ref().is_ok_and(|paths| paths.is_default(&path))
    {
        return instance::show_existing(&path);
    }
    let paths = paths?;
    // 旧版仍运行时只唤回，不搬动它正在使用的数据库或附件。
    Ok(instance::show_existing(&paths.current)? || instance::show_existing(&paths.legacy)?)
}

#[cfg(test)]
mod tests;
