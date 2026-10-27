use anyhow::{Context, Result, bail};
use std::{fs, io::Write, path::Path};

pub(super) fn read(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Cannot read the autostart entry"),
    }
}

pub(super) fn write(path: &Path, content: &str) -> Result<()> {
    let directory = path.parent().context("Invalid autostart directory")?;
    fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(content.as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .context("Cannot save the autostart entry")?;
    if read(path)?.as_deref() != Some(content) {
        bail!("The saved autostart entry could not be verified");
    }
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

pub(super) fn migrate(old: &Path, new: &Path, saved: &str, replacement: &str) -> Result<()> {
    match read(new)? {
        Some(value) if value == replacement => (),
        Some(_) => bail!("The destination autostart entry changed during migration"),
        None => {
            let directory = new.parent().context("Invalid autostart directory")?;
            fs::create_dir_all(directory)?;
            let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
            temporary.write_all(replacement.as_bytes())?;
            temporary.as_file().sync_all()?;
            temporary
                .persist_noclobber(new)
                .context("Cannot create the new autostart entry")?;
        }
    }
    if read(new)?.as_deref() != Some(replacement) {
        bail!("The new autostart entry could not be verified");
    }
    #[cfg(unix)]
    fs::File::open(new.parent().context("Invalid autostart directory")?)?.sync_all()?;
    if read(old)?.as_deref() != Some(saved) {
        bail!("The old autostart entry changed during migration");
    }
    fs::remove_file(old).context("Cannot remove the old autostart entry")?;
    #[cfg(unix)]
    fs::File::open(old.parent().context("Invalid autostart directory")?)?.sync_all()?;
    Ok(())
}
