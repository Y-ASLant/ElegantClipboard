use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn entry_path(data_dir: &Path) -> Result<PathBuf> {
    let canonical = data_dir
        .canonicalize()
        .context("Cannot resolve the data directory")?;
    use std::os::unix::ffi::OsStrExt;
    let hash = clipboard_core::backup::hash_token(canonical.as_os_str().as_bytes());
    let config = match std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        _ => PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".config"),
    };
    Ok(config
        .join("autostart")
        .join(format!("elegant-clipboard-{}.desktop", &hash[..16])))
}

fn quote(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .context("Autostart paths must be valid UTF-8")?;
    if value.contains(['\n', '\r', '\0']) {
        bail!("Autostart paths cannot contain line breaks or NUL");
    }
    // Desktop Entry Exec quoting is not shell quoting. Escape percent field codes,
    // then the reserved characters inside double quotes, then the string layer.
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '%' => quoted.push_str("%%"),
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(quoted)
}

fn entry(data_dir: &Path) -> Result<String> {
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ElegantClipboard\nExec={} --start-hidden --data-dir {}\nTerminal=false\n",
        quote(&std::env::current_exe()?)?,
        quote(&data_dir.canonicalize()?)?
    ))
}

pub fn enabled(data_dir: &Path) -> Result<bool> {
    match fs::read_to_string(entry_path(data_dir)?) {
        Ok(saved) => Ok(saved == entry(data_dir)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub fn set_enabled(data_dir: &Path, enabled: bool) -> Result<()> {
    let path = entry_path(data_dir)?;
    if enabled {
        let content = entry(data_dir)?;
        fs::create_dir_all(path.parent().context("Invalid autostart directory")?)?;
        let temporary = path.with_extension(format!("desktop.{}.tmp", std::process::id()));
        fs::write(&temporary, content)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(temporary);
            return Err(error.into());
        }
    } else if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    Ok(())
}
