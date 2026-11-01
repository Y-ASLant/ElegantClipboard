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
