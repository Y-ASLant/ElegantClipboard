//! Per-profile LaunchAgent, loaded by launchd at the next graphical login.
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "../unix/autostart_file.rs"]
mod file;
#[path = "autostart/plist.rs"]
mod plist;
#[cfg(test)]
#[path = "autostart/tests.rs"]
mod tests;

fn launch_agents() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("macOS HOME 环境变量缺失")?)
            .join("Library/LaunchAgents"),
    )
}

fn label(data_dir: &Path) -> Result<String> {
    if !data_dir.is_absolute() {
        bail!("登录启动数据目录必须为绝对路径");
    }
    let hash = clipboard_core::backup::hash_token(data_dir.as_os_str().as_encoded_bytes());
    Ok(format!("app.elegantclipboard.{}", &hash[..16]))
}

fn entry_path(base: &Path, label: &str) -> PathBuf {
    base.join(format!("{label}.plist"))
}

fn entry(exe: &Path, data_dir: &Path) -> Result<String> {
    let label = label(data_dir)?;
    let arguments = plist::arguments(
        exe.to_str().context("程序路径不是有效 UTF-8")?,
        data_dir.to_str().context("数据目录不是有效 UTF-8")?,
    );
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>{label}</string>\n\
         <key>ProgramArguments</key>{arguments}\n\
         <key>RunAtLoad</key><true/>\n\
         <key>LimitLoadToSessionType</key><string>Aqua</string>\n\
         </dict></plist>\n"
    ))
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let label = label(&canonical)?;
    let executable = std::env::current_exe()?;
    Ok(file::read(&entry_path(&launch_agents()?, &label))?
        .and_then(|saved| plist::parse(&saved, &label, &canonical))
        .is_some_and(|entry| entry.enabled && entry.executable == executable))
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let canonical = data_dir.canonicalize().context("无法解析数据目录")?;
    let path = entry_path(&launch_agents()?, &label(&canonical)?);
    if enabled {
        file::write(&path, &entry(&std::env::current_exe()?, &canonical)?)?;
    } else if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error).context("无法关闭 macOS 登录启动");
    }
    Ok(())
}

pub(crate) fn migrate_data_dir(old_dir: &Path, new_dir: &Path) -> Result<()> {
    migrate_at(
        &launch_agents()?,
        old_dir,
        new_dir,
        &std::env::current_exe()?,
    )
}

fn migrate_at(base: &Path, old_dir: &Path, new_dir: &Path, exe: &Path) -> Result<()> {
    let old_label = label(old_dir)?;
    let new_label = label(new_dir)?;
    let old = entry_path(base, &old_label);
    let new = entry_path(base, &new_label);
    if old == new {
        return Ok(());
    }
    let Some(saved) = file::read(&old)? else {
        return Ok(());
    };
    let Some(parsed) = plist::parse(&saved, &old_label, old_dir) else {
        return Ok(());
    };
    let replacement = match file::read(&new)? {
        Some(value) if plist::parse(&value, &new_label, new_dir).is_some() => value,
        Some(_) => bail!("目标登录启动设置被其他程序占用"),
        None => plist::relocate(
            &saved,
            &parsed,
            &new_label,
            exe.to_str().context("程序路径不是有效 UTF-8")?,
            new_dir.to_str().context("数据目录不是有效 UTF-8")?,
        ),
    };
    file::migrate(&old, &new, &saved, &replacement)
}
