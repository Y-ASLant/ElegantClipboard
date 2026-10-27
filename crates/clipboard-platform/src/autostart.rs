//! Current-user Windows startup entries, with compatibility for the GPUI value name.

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

fn names(data_dir: &Path) -> Result<[String; 2]> {
    if !data_dir.is_absolute() {
        bail!("开机启动数据目录必须为绝对路径");
    }
    let normalized = data_dir.to_string_lossy().to_lowercase();
    let hash = clipboard_core::backup::hash_token(normalized.as_bytes());
    Ok([
        format!("ElegantClipboard-{}", &hash[..16]),
        format!("ElegantClipboard-GPUI-{}", &hash[..16]),
    ])
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
    matches!(
        executable.as_deref(),
        Some("elegant-clipboard-gpui.exe" | "elegantclipboard.exe")
    ) && !data.contains(['"', '\0'])
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
    for name in names(&canonical)? {
        if let Some(value) = key.read(&name)?
            && value.string().as_deref() == Some(&expected)
        {
            return Ok(true);
        }
    }
    Ok(false)
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

fn copy_approval(sources: &[String], destination: &str) -> Result<()> {
    let Some(key) = registry::Key::open(APPROVAL_KEY, false)? else {
        return Ok(());
    };
    if key.read(destination)?.is_some() {
        return Ok(());
    }
    for source in sources {
        if let Some(value) = key.read(source)? {
            registry::Key::open(APPROVAL_KEY, true)?
                .context("无法打开开机启动状态")?
                .write_verified(destination, &value)?;
            break;
        }
    }
    Ok(())
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let Some(key) = registry::Key::open(RUN_KEY, true)? else {
        return Ok(());
    };
    let [name, legacy] = names(&canonical)?;
    if enabled {
        if let Some(saved) = key.read(&name)?
            && !belongs_to(&saved, &canonical)
        {
            bail!("开机启动设置被其他程序占用");
        }
        if let Some(saved) = key.read(&legacy)?
            && belongs_to(&saved, &canonical)
        {
            copy_approval(std::slice::from_ref(&legacy), &name)?;
        }
        key.write_verified(&name, &startup_value(&canonical)?)?;
    }
    let candidates = if enabled {
        vec![legacy]
    } else {
        vec![name, legacy]
    };
    for name in candidates {
        if let Some(value) = key.read(&name)?
            && belongs_to(&value, &canonical)
        {
            key.remove(&name)?;
            remove_approval(&name)?;
        }
    }
    Ok(())
}

pub(crate) fn migrate_data_dir(old_dir: &Path, new_dir: &Path) -> Result<()> {
    if old_dir == new_dir {
        return Ok(());
    }
    let Some(key) = registry::Key::open(RUN_KEY, false)? else {
        return Ok(());
    };
    let [new_name, _] = names(new_dir)?;
    let mut sources = Vec::new();
    for name in names(old_dir)? {
        if let Some(value) = key.read(&name)?
            && belongs_to(&value, old_dir)
        {
            sources.push((name, value));
        }
    }
    if sources.is_empty() {
        return Ok(());
    }
    let destination = key.read(&new_name)?;
    if let Some(value) = &destination
        && !belongs_to(value, new_dir)
    {
        bail!("目标开机启动设置被其他程序占用");
    }
    let writable = registry::Key::open(RUN_KEY, true)?.context("无法打开开机启动设置")?;
    let expected = match destination {
        Some(value) => value,
        None => {
            let source_names: Vec<_> = sources.iter().map(|(name, _)| name.clone()).collect();
            copy_approval(&source_names, &new_name)?;
            let value = startup_value(new_dir)?;
            if writable.read(&new_name)?.is_some() {
                bail!("目标开机启动设置在迁移时已变化");
            }
            writable.write_verified(&new_name, &value)?;
            value
        }
    };
    for (name, saved) in sources {
        if writable.read(&new_name)?.as_ref() != Some(&expected)
            || key.read(&name)?.as_ref() != Some(&saved)
        {
            bail!("开机启动设置在迁移时已变化");
        }
        writable.remove(&name)?;
        remove_approval(&name)?;
    }
    Ok(())
}
