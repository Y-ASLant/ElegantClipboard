use super::{DataDirectory, DefaultPaths, lock_instance, same_path};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const MARKER: &str = ".data-directory-migration.json";

#[derive(Serialize, Deserialize)]
struct PendingMigration {
    previous_roots: Vec<PathBuf>,
    paths_updated: bool,
}

pub(super) fn prepare_default(
    paths: &DefaultPaths,
    migrate_autostart: impl Fn(&Path, &Path) -> Result<()>,
) -> Result<DataDirectory> {
    let parent = paths.current.parent().context("数据目录缺少父目录")?;
    fs::create_dir_all(parent).context("无法创建应用数据父目录")?;
    // 锁文件保留原 inode，避免并发进程分别锁住已删除、重建的两个文件。
    let gate_path = parent.join(".ElegantClipboard-migration.lock");
    reject_link(&gate_path)?;
    let gate = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&gate_path)?;
    gate.try_lock()
        .map_err(|_| super::super::data::InstanceBusy)?;
    reject_link(&paths.current)?;
    reject_link(&paths.legacy)?;

    reject_link(&paths.current.join(MARKER))?;
    if paths.current.join(MARKER).is_file() {
        let lock = lock_instance(&paths.current)?;
        validate_tree(&paths.current)?;
        return finish(paths, lock, migrate_autostart);
    }
    let old_exists = paths.legacy.is_dir();
    let current_empty = !paths.current.exists() || only_lock(&paths.current)?;
    if old_exists && current_empty && paths.legacy.join("clipboard.db").is_file() {
        let lock = lock_instance(&paths.legacy)
            .context("旧版仍在运行或数据目录无法锁定，请退出旧版后重试")?;
        validate_tree(&paths.legacy)?;
        let roots = previous_roots(&paths.legacy)?;
        // 标记先随旧目录一起移动，中断后可分辨已安装与尚待安装的副本。
        write_marker(
            &paths.legacy,
            &PendingMigration {
                previous_roots: roots,
                paths_updated: false,
            },
        )?;
        sync_directory(&paths.legacy)?;
        if paths.current.exists() {
            let current_lock = lock_instance(&paths.current)?;
            drop(current_lock);
            remove_empty_profile(&paths.current)?;
        }
        // Windows 不允许移动含已锁文件的目录。迁移互斥锁保持有效，
        // 释放旧文件句柄后移动，再在访问数据库前立即锁住新目录。
        #[cfg(windows)]
        drop(lock);
        fs::rename(&paths.legacy, &paths.current).context("无法迁移旧数据目录，原数据已保留")?;
        sync_directory(parent)?;
        sync_directory(paths.legacy.parent().context("旧数据目录缺少父目录")?)?;
        #[cfg(windows)]
        let lock = lock_instance(&paths.current)?;
        return finish(paths, lock, migrate_autostart);
    }
    let lock = lock_instance(&paths.current)?;
    let warning = if old_exists && !only_lock(&paths.legacy)? {
        Some(format!(
            "新旧数据目录均有内容，已使用 {}，并保留 {}；请通过备份/导入处理，未覆盖或删除旧历史。",
            paths.current.display(),
            paths.legacy.display()
        ))
    } else {
        if old_exists {
            let old_lock = lock_instance(&paths.legacy)?;
            let old_canonical = paths.legacy.canonicalize()?;
            let current_canonical = paths.current.canonicalize()?;
            if let Err(error) = migrate_autostart(&old_canonical, &current_canonical) {
                return Ok(DataDirectory {
                    path: paths.current.clone(),
                    lock,
                    warning: Some(format!(
                        "旧目录为空，但开机启动路径更新失败，已保留旧目录供下次重试：{error:#}"
                    )),
                });
            }
            drop(old_lock);
            remove_empty_profile(&paths.legacy)?;
            remove_empty_parents(&paths.legacy, parent);
        }
        None
    };
    Ok(DataDirectory {
        path: paths.current.clone(),
        lock,
        warning,
    })
}

fn finish(
    paths: &DefaultPaths,
    lock: File,
    migrate_autostart: impl Fn(&Path, &Path) -> Result<()>,
) -> Result<DataDirectory> {
    let marker_path = paths.current.join(MARKER);
    reject_link(&marker_path)?;
    anyhow::ensure!(
        fs::metadata(&marker_path)?.len() <= 32 * 1024,
        "数据迁移标记超过允许大小"
    );
    let bytes = fs::read(&marker_path)?;
    let mut pending: PendingMigration =
        serde_json::from_slice(&bytes).context("数据迁移标记损坏，已保留数据")?;
    let legacy_parent = paths.legacy.parent().context("旧数据目录缺少父目录")?;
    let canonical_legacy = legacy_parent
        .canonicalize()
        .ok()
        .map(|parent| parent.join(paths.legacy.file_name().unwrap_or_default()));
    anyhow::ensure!(
        !pending.previous_roots.is_empty()
            && pending.previous_roots.len() <= 2
            && pending
                .previous_roots
                .iter()
                .all(|root| same_path(root, &paths.legacy)
                    || canonical_legacy.as_deref() == Some(root.as_path())),
        "迁移来源与旧版默认目录不匹配"
    );
    let warning = (|| -> Result<()> {
        if !pending.paths_updated {
            clipboard_core::relocate_managed_paths(&paths.current, &pending.previous_roots)?;
            pending.paths_updated = true;
            write_marker(&paths.current, &pending)?;
        }
        let current = paths.current.canonicalize()?;
        for previous in &pending.previous_roots {
            migrate_autostart(previous, &current)?;
        }
        if paths.legacy.exists() {
            bail!(
                "旧目录已被其他程序重新创建，未清理：{}",
                paths.legacy.display()
            );
        }
        fs::remove_file(&marker_path).context("无法移除迁移完成标记")?;
        sync_directory(&paths.current)?;
        remove_empty_parents(&paths.legacy, paths.current.parent().unwrap());
        Ok(())
    })();
    match warning {
        Ok(()) => Ok(DataDirectory {
            path: paths.current.clone(),
            lock,
            warning: None,
        }),
        Err(error) if pending.paths_updated => Ok(DataDirectory {
            path: paths.current.clone(),
            lock,
            warning: Some(format!(
                "历史数据已迁移；启动项或旧目录清理尚未完成，将在下次启动重试：{error:#}"
            )),
        }),
        Err(error) => Err(error).context("历史数据路径迁移失败，数据保留在新目录，修复后可重试"),
    }
}

fn previous_roots(path: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = vec![path.to_owned()];
    let canonical = path.canonicalize()?;
    if canonical != path {
        roots.push(canonical);
    }
    Ok(roots)
}

fn write_marker(directory: &Path, marker: &PendingMigration) -> Result<()> {
    let marker_path = directory.join(MARKER);
    reject_link(&marker_path)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(temporary.as_file_mut(), marker)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(&marker_path)
        .map_err(|error| error.error)?;
    sync_directory(directory)
}

fn sync_directory(directory: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

fn only_lock(directory: &Path) -> Result<bool> {
    Ok(fs::read_dir(directory)?.all(|entry| {
        entry.is_ok_and(|entry| {
            entry.file_name() == "instance.lock"
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
    }))
}

fn remove_empty_profile(directory: &Path) -> Result<()> {
    anyhow::ensure!(
        only_lock(directory)?,
        "目录出现其他内容，未清理：{}",
        directory.display()
    );
    match fs::remove_file(directory.join("instance.lock")) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    fs::remove_dir(directory).context("无法清理旧的空数据目录")?;
    sync_directory(directory.parent().context("旧目录缺少父目录")?)
}

fn remove_empty_parents(legacy: &Path, boundary: &Path) {
    let mut parent = legacy.parent();
    while let Some(path) = parent {
        if same_path(path, boundary) || !path.starts_with(boundary) {
            break;
        }
        if fs::remove_dir(path).is_err() {
            break;
        }
        parent = path.parent();
    }
}

fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let linked = metadata.file_type().is_symlink();
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                linked || metadata.file_attributes() & 0x400 != 0
            };
            anyhow::ensure!(
                !linked,
                "数据迁移不跟随符号链接或重解析点：{}",
                path.display()
            );
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        reject_link(&directory)?;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            reject_link(&entry.path())?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}
