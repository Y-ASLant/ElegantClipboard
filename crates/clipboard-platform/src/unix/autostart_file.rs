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
