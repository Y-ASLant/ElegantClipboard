use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "autostart/desktop.rs"]
mod desktop;
#[path = "../unix/autostart_file.rs"]
mod file;
#[cfg(test)]
#[path = "autostart/tests.rs"]
mod tests;

fn configuration_base() -> Result<PathBuf> {
    Ok(
        match std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
            Some(path) if path.is_absolute() => path,
            _ => {
                PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".config")
            }
        },
    )
}

fn entry_path(base: &Path, data_dir: &Path) -> Result<PathBuf> {
    if !data_dir.is_absolute() {
        bail!("Autostart data directories must be absolute");
    }
    let hash = clipboard_core::backup::hash_token(data_dir.as_os_str().as_encoded_bytes());
    Ok(base
        .join("autostart")
        .join(format!("elegant-clipboard-{}.desktop", &hash[..16])))
}

fn entry(exe: &Path, data_dir: &Path) -> Result<String> {
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ElegantClipboard\n{}\nTerminal=false\n",
        desktop::exec(exe, data_dir)?
    ))
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    let canonical = data_dir
        .canonicalize()
        .context("Cannot resolve the data directory")?;
    let executable = std::env::current_exe()?;
    Ok(
        file::read(&entry_path(&configuration_base()?, &canonical)?)?
            .and_then(|saved| desktop::parse(&saved, &canonical))
            .is_some_and(|entry| entry.enabled && entry.executable == executable),
    )
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let canonical = data_dir
        .canonicalize()
        .context("Cannot resolve the data directory")?;
    let path = entry_path(&configuration_base()?, &canonical)?;
    if enabled {
        file::write(&path, &entry(&std::env::current_exe()?, &canonical)?)?;
    } else if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    Ok(())
}

pub(crate) fn migrate_data_dir(old_dir: &Path, new_dir: &Path) -> Result<()> {
    migrate_at(
        &configuration_base()?,
        old_dir,
        new_dir,
        &std::env::current_exe()?,
    )
}

fn migrate_at(base: &Path, old_dir: &Path, new_dir: &Path, exe: &Path) -> Result<()> {
    let old = entry_path(base, old_dir)?;
    let new = entry_path(base, new_dir)?;
    if old == new {
        return Ok(());
    }
    let Some(saved) = file::read(&old)? else {
        return Ok(());
    };
    let Some(parsed) = desktop::parse(&saved, old_dir) else {
        return Ok(());
    };
    let replacement = match file::read(&new)? {
        Some(value) if desktop::parse(&value, new_dir).is_some() => value,
        Some(_) => bail!("The destination autostart entry belongs to another application"),
        None => desktop::relocate(&saved, &parsed, exe, new_dir)?,
    };
    file::migrate(&old, &new, &saved, &replacement)
}
