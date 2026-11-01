//! Current-user Windows startup entries.

use crate::instance::{to_wide, to_wide_bytes};
use anyhow::{Context, Result, bail};
use std::path::Path;
use windows::Win32::System::Registry::REG_SZ;

mod registry;
#[cfg(test)]
mod tests;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPROVAL_KEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";

fn startup_name(data_dir: &Path) -> Result<String> {
    if !data_dir.is_absolute() {
        bail!("开机启动数据目录必须为绝对路径");
    }
    let normalized = data_dir.to_string_lossy().to_lowercase();
    let hash = clipboard_core::backup::hash_token(normalized.as_bytes());
    Ok(format!("ElegantClipboard-{}", &hash[..16]))
}

fn command(exe: &Path, data_dir: &Path) -> String {
    format!(
        "\"{}\" --start-hidden --data-dir \"{}\"",
        exe.display(),
        data_dir.display()
    )
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(unc) = value.strip_prefix("\\\\?\\unc\\") {
        format!("\\\\{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(&value).to_owned()
    }
}

fn belongs_to(value: &registry::Value, data_dir: &Path) -> bool {
    let Some(saved) = value.string() else {
        return false;
    };
    let Some((exe, data)) = saved
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.split_once("\" --start-hidden --data-dir \""))
    else {
        return false;
    };
    let executable = Path::new(exe);
    if !executable.is_absolute() || exe.contains(['"', '\0']) {
        return false;
    }
    let executable = executable
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase());
    executable.as_deref() == Some("elegant-clipboard-gpui.exe")
        && !data.contains(['"', '\0'])
        && path_key(Path::new(data)) == path_key(data_dir)
}

fn startup_value(data_dir: &Path) -> Result<registry::Value> {
    Ok(registry::Value {
        kind: REG_SZ,
        bytes: to_wide_bytes(&command(&std::env::current_exe()?, data_dir)),
    })
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let Some(key) = registry::Key::open(RUN_KEY, false)? else {
        return Ok(false);
    };
    let expected = command(&std::env::current_exe()?, &canonical);
    let entry_name = startup_name(&canonical)?;
    Ok(key
        .read(&entry_name)?
        .and_then(|value| value.string())
        .as_deref()
        == Some(expected.as_str()))
}

fn remove_approval(name: &str) -> Result<()> {
    if let Some(key) = registry::Key::open(APPROVAL_KEY, false)?
        && key.read(name)?.is_some()
    {
        registry::Key::open(APPROVAL_KEY, true)?
            .context("无法打开开机启动状态")?
            .remove(name)?;
    }
    Ok(())
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let Some(key) = registry::Key::open(RUN_KEY, true)? else {
        return Ok(());
    };
    let entry_name = startup_name(&canonical)?;
    if enabled {
        if let Some(saved) = key.read(&entry_name)?
            && !belongs_to(&saved, &canonical)
        {
            bail!("开机启动设置被其他程序占用");
        }
        key.write_verified(&entry_name, &startup_value(&canonical)?)?;
    } else if let Some(value) = key.read(&entry_name)?
        && belongs_to(&value, &canonical)
    {
        key.remove(&entry_name)?;
        remove_approval(&entry_name)?;
    }
    Ok(())
}
