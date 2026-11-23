use super::data::lock_instance;
use crate::instance;
use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

pub(super) struct DataDirectory {
    pub(super) path: PathBuf,
    pub(super) lock: File,
}

const LOCATION_FILE: &str = "data-location";
const PENDING_IMPORT_FILE: &str = ".pending-import";
const PENDING_RELOCATE_FILE: &str = ".pending-relocate";

/// 固定控制目录：默认数据根，也存放自定义路径指针与待处理任务。
pub fn control_root() -> Result<PathBuf> {
    let base = BaseDirs::new().context("无法确定用户数据目录")?;
    #[cfg(target_os = "linux")]
    let name = "elegantclipboard";
    #[cfg(not(target_os = "linux"))]
    let name = "ElegantClipboard";
    Ok(base.data_local_dir().join(name))
}

fn path_key(path: &Path) -> String {
    display_path(path).to_lowercase()
}

/// UI / 配置文件用路径：去掉 Windows `\\?\` 扩展前缀。
pub fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let normalized = raw.replace('/', "\\");
    if let Some(rest) = normalized
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| normalized.strip_prefix(r"//?/UNC/"))
    {
        format!(r"\\{rest}")
    } else if let Some(rest) = normalized
        .strip_prefix(r"\\?\")
        .or_else(|| normalized.strip_prefix(r"//?/"))
    {
        rest.to_string()
    } else {
        raw.into_owned()
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| path_key(&a) == path_key(&b))
}

/// 环境变量 / 显式路径优先；否则读控制目录中的 data-location；再否则控制目录本身。
pub(super) fn resolve(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_owned());
    }
    let root = control_root()?;
    let location = root.join(LOCATION_FILE);
    if location.is_file() {
        let raw = fs::read_to_string(&location).context("无法读取自定义数据目录配置")?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    Ok(root)
}

/// 将自定义数据目录写入控制根。目标等于控制根时删除指针，恢复默认。
pub fn set_data_location(path: &Path) -> Result<()> {
    let root = control_root()?;
    fs::create_dir_all(&root).context("无法创建数据控制目录")?;
    let location = root.join(LOCATION_FILE);
    if same_path(path, &root) {
        let _ = fs::remove_file(&location);
        return Ok(());
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        fs::canonicalize(path).context("无法解析自定义数据目录")?
    };
    let stored = display_path(&absolute);
    fs::write(&location, stored.as_bytes()).context("无法保存自定义数据目录")?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingImport {
    pub source: PathBuf,
    pub kind: String,
}

impl PendingImport {
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRelocate {
    to: PathBuf,
}

pub fn queue_pending_import(data_dir: &Path, source: &Path, kind: &str) -> Result<()> {
    let source = fs::canonicalize(source).context("无法解析备份文件路径")?;
    if !source.is_file() {
        bail!("备份文件不存在");
    }
    fs::create_dir_all(data_dir).context("无法创建数据目录")?;
    let job = PendingImport {
        source,
        kind: kind.to_owned(),
    };
    let path = data_dir.join(PENDING_IMPORT_FILE);
    fs::write(&path, serde_json::to_vec_pretty(&job)?).context("无法写入导入任务")?;
    Ok(())
}

pub fn queue_pending_relocate(to: &Path) -> Result<()> {
    let root = control_root()?;
    fs::create_dir_all(&root).context("无法创建数据控制目录")?;
    let from = resolve(None)?;
    fs::create_dir_all(to).context("无法创建目标数据目录")?;
    let to = fs::canonicalize(to).context("无法解析目标数据目录")?;
    if same_path(&from, &to) {
        bail!("目标与当前保存位置相同，请选择其他空目录");
    }
    directory_is_usable_target(&to)?;
    let job = PendingRelocate { to };
    fs::write(
        root.join(PENDING_RELOCATE_FILE),
        serde_json::to_vec_pretty(&job)?,
    )
    .context("无法写入迁移任务")?;
    Ok(())
}

fn managed_names() -> &'static [&'static str] {
    &[
        "clipboard.db",
        "clipboard.db-wal",
        "clipboard.db-shm",
        "images",
        "icons",
        "staged",
        "blobs",
    ]
}

fn remove_managed_with_retry(data_dir: &Path, progress: &mut dyn FnMut(f32, &str)) -> Result<()> {
    use std::{thread, time::Duration};
    for name in managed_names() {
        let path = data_dir.join(name);
        if !path.exists() {
            continue;
        }
        let mut last_error = None::<std::io::Error>;
        for attempt in 0..40 {
            let result = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            match result {
                Ok(()) => {
                    last_error = None;
                    break;
                }
                Err(_) if !path.exists() => {
                    // 竞态下目标已消失
                    last_error = None;
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt % 5 == 0 {
                        progress(3.0, "数据文件仍被占用，正在重试");
                    }
                    thread::sleep(Duration::from_millis(150));
                }
            }
        }
        if let Some(error) = last_error {
            return Err(error).with_context(|| {
                format!(
                    "无法删除旧数据：{}（请先完全退出 ElegantClipboard 及其他打开该库的程序）",
                    path.display()
                )
            });
        }
    }
    Ok(())
}

fn is_ignorable_target_entry(name: &str) -> bool {
    matches!(
        name,
        "instance.lock"
            | ".pending-import"
            | ".pending-relocate"
            | "data-location"
            | "logs"
            | ".wait-for-instance"
            // Windows / macOS 空目录里常见的系统垃圾文件，不视为“非空”。
            | "desktop.ini"
            | "Thumbs.db"
            | ".DS_Store"
            | "ehthumbs.db"
    )
}

fn directory_is_usable_target(path: &Path) -> Result<()> {
    fs::create_dir_all(path).context("无法创建目标目录")?;
    for entry in fs::read_dir(path).context("无法读取目标目录")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_ignorable_target_entry(name.as_ref()) {
            continue;
        }
        if managed_names().iter().any(|managed| *managed == name) {
            bail!("目标目录已有剪贴板数据，请选择空目录或先清空");
        }
        bail!("目标目录非空，请选择空目录");
    }
    Ok(())
}

/// 等旧实例释放 instance.lock（进程退出后 SQLite 句柄也会关掉，才能搬库）。
fn wait_until_instance_released(data_dir: &Path) -> Result<()> {
    use std::{fs::OpenOptions, thread, time::Duration};
    fs::create_dir_all(data_dir).context("无法创建数据目录")?;
    let lock_path = data_dir.join("instance.lock");
    for attempt in 0..100 {
        let released = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .ok()
            .and_then(|file| file.try_lock().ok().map(|_| file))
            .is_some();
        if released {
            return Ok(());
        }
        if attempt + 1 == 100 {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    bail!("等待旧进程退出超时，请关闭已运行的实例后重试")
}

fn move_entry(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目标父目录：{}", parent.display()))?;
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            // 跨盘符时 rename 会失败：退化为复制再删。
            if source.is_dir() {
                copy_dir_recursive(source, destination).with_context(|| {
                    format!(
                        "无法复制目录 {} → {}（rename: {rename_error})",
                        source.display(),
                        destination.display()
                    )
                })?;
                fs::remove_dir_all(source)
                    .with_context(|| format!("已复制但无法删除源目录：{}", source.display()))?;
            } else {
                fs::copy(source, destination).with_context(|| {
                    format!(
                        "无法复制文件 {} → {}（rename: {rename_error})",
                        source.display(),
                        destination.display()
                    )
                })?;
                fs::remove_file(source)
                    .with_context(|| format!("已复制但无法删除源文件：{}", source.display()))?;
            }
            Ok(())
        }
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn apply_pending_relocate() -> Result<Option<PathBuf>> {
    let root = control_root()?;
    let job_path = root.join(PENDING_RELOCATE_FILE);
    if !job_path.is_file() {
        return Ok(None);
    }
    let job: PendingRelocate =
        serde_json::from_slice(&fs::read(&job_path).context("无法读取迁移任务")?)
            .context("迁移任务损坏")?;
    let from = resolve(None)?;
    let to = job.to;
    if same_path(&from, &to) {
        let _ = fs::remove_file(&job_path);
        set_data_location(&to)?;
        return Ok(Some(to));
    }
    // 永久错误（非空等）：清任务，避免每次启动都失败。
    if let Err(error) = directory_is_usable_target(&to) {
        let _ = fs::remove_file(&job_path);
        return Err(error);
    }
    wait_until_instance_released(&from)?;
    for name in managed_names() {
        let source = from.join(name);
        if !source.exists() {
            continue;
        }
        let destination = to.join(name);
        move_entry(&source, &destination)
            .with_context(|| format!("无法移动 {name} 到新数据目录"))?;
    }
    let logs = from.join("logs");
    if logs.is_dir() && !to.join("logs").exists() {
        let _ = move_entry(&logs, &to.join("logs"));
    }
    set_data_location(&to)?;
    let _ = fs::remove_file(&job_path);
    Ok(Some(to))
}

pub fn has_pending_import(data_dir: &Path) -> bool {
    data_dir.join(PENDING_IMPORT_FILE).is_file()
}

pub fn apply_pending_import(data_dir: &Path) -> Result<Option<String>> {
    apply_pending_import_with_progress(data_dir, &mut |_, _| {})
}

/// `progress(percent 0..=100, label)` 供启动进度窗刷新。
pub fn apply_pending_import_with_progress(
    data_dir: &Path,
    progress: &mut dyn FnMut(f32, &str),
) -> Result<Option<String>> {
    let job_path = data_dir.join(PENDING_IMPORT_FILE);
    if !job_path.is_file() {
        return Ok(None);
    }
    let job: PendingImport =
        serde_json::from_slice(&fs::read(&job_path).context("无法读取导入任务")?)
            .context("导入任务损坏")?;
    // 无论成败都清任务，避免启动反复弹失败窗。
    let clear_job = || {
        let _ = fs::remove_file(&job_path);
    };

    progress(1.0, "等待其他实例释放数据目录");
    // 旧进程占着 clipboard.db 时删除会 os error 32。
    if let Err(error) = wait_until_instance_released(data_dir) {
        clear_job();
        return Err(error);
    }
    progress(3.0, "清理当前数据");
    if let Err(error) = remove_managed_with_retry(data_dir, progress) {
        clear_job();
        return Err(error);
    }

    let result = match job.kind.as_str() {
        "legacy" => {
            let report = clipboard_core::legacy_backup::import_legacy_backup_with_progress(
                &job.source,
                data_dir,
                progress,
            );
            report.map(|report| Some(format!("已导入旧版数据 {} 条", report.items.total_items)))
        }
        "gpui" => {
            let report = clipboard_core::backup::restore_backup_with_progress(
                &job.source,
                data_dir,
                progress,
            );
            report.map(|report| Some(format!("已恢复备份 {} 条", report.total_items)))
        }
        other => Err(anyhow::anyhow!("未知导入类型：{other}")),
    };
    clear_job();
    result
}

/// 启动前只处理目录迁移；导入改在可见进度窗执行。
pub fn apply_startup_jobs(explicit: Option<&Path>) -> Result<PathBuf> {
    let _ = apply_pending_relocate()?;
    resolve(explicit)
}

pub(super) fn prepare(data_dir: Option<PathBuf>) -> Result<DataDirectory> {
    let path = resolve(data_dir.as_deref())?;
    let lock = lock_instance(&path)?;
    Ok(DataDirectory { path, lock })
}

pub(super) fn show_existing(data_dir: Option<PathBuf>) -> Result<bool> {
    instance::show_existing(&resolve(data_dir.as_deref())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn custom_location_round_trip() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().join("ElegantClipboard");
        fs::create_dir_all(&root)?;
        let custom = temporary.path().join("custom-data");
        fs::create_dir_all(&custom)?;
        fs::write(
            root.join(LOCATION_FILE),
            custom.to_string_lossy().as_bytes(),
        )?;
        let raw = fs::read_to_string(root.join(LOCATION_FILE))?;
        assert_eq!(PathBuf::from(raw.trim()), custom);
        Ok(())
    }

    #[test]
    fn empty_target_allows_desktop_ini() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let target = temporary.path().join("empty");
        fs::create_dir_all(&target)?;
        fs::write(target.join("desktop.ini"), b"[.ShellClassInfo]")?;
        directory_is_usable_target(&target)?;
        Ok(())
    }

    #[test]
    fn display_path_strips_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\S:\data")), r"S:\data");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            r"\\server\share"
        );
    }

    #[test]
    fn move_entry_copies_when_needed() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("src.txt");
        let dest = temporary.path().join("nested").join("dst.txt");
        fs::write(&source, b"payload")?;
        move_entry(&source, &dest)?;
        assert_eq!(fs::read(&dest)?, b"payload");
        assert!(!source.exists());
        Ok(())
    }
}
