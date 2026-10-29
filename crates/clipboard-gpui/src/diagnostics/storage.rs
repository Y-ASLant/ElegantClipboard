use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::fmt::MakeWriter;

pub(super) const MAX_EVENT_BYTES: usize = 1024 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const BACKUPS: usize = 3;
const INACTIVE_SESSIONS: usize = 5;
const TRUNCATED: &[u8] = b"\n[diagnostics: event exceeded 1 MiB; remainder omitted]\n";

pub(super) struct Session {
    pub(super) directory: PathBuf,
    root: PathBuf,
    lease: Option<File>,
}

impl Session {
    pub(super) fn create(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let _guard = retention_lock(root)?;
        prune(root)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        for sequence in 0..1000 {
            let directory = root.join(format!(
                "session-{timestamp:030}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            let lease = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join("active.lock"))?;
            lease.lock()?;
            return Ok(Self {
                directory,
                root: root.to_owned(),
                lease: Some(lease),
            });
        }
        Err(io::Error::other("could not allocate a diagnostic session"))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let result = (|| {
            let _guard = retention_lock(&self.root)?;
            drop(self.lease.take());
            prune(&self.root)
        })();
        if let Err(error) = result {
            eprintln!("ElegantClipboard diagnostic retention failed: {error}");
        }
    }
}

fn retention_lock(root: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join("retention.lock"))?;
    file.lock()?;
    Ok(file)
}

// Caller holds retention.lock across creation, lease release, and cleanup.
fn prune(root: &Path) -> io::Result<()> {
    let mut inactive = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !is_session_name(&entry.file_name()) {
            continue;
        }
        let lease = match OpenOptions::new()
            .write(true)
            .open(entry.path().join("active.lock"))
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::remove_dir_all(entry.path())?;
                continue;
            }
            Err(error) => return Err(error),
        };
        match lease.try_lock() {
            Ok(()) => inactive.push((entry.file_name(), entry.path(), lease)),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    inactive.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    for (_, path, lease) in inactive.into_iter().skip(INACTIVE_SESSIONS) {
        drop(lease);
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn is_session_name(name: &std::ffi::OsStr) -> bool {
    let Some(suffix) = name.to_str().and_then(|name| name.strip_prefix("session-")) else {
        return false;
    };
    let mut fields = suffix.split('-');
    let (Some(timestamp), Some(pid), Some(sequence), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return false;
    };
    timestamp.len() == 30
        && [timestamp, pid, sequence]
            .iter()
            .all(|field| !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit()))
}

pub(super) struct RotatingFile {
    directory: PathBuf,
    file: Option<File>,
    bytes: u64,
}

impl RotatingFile {
    pub(super) fn open(directory: &Path) -> io::Result<Self> {
        let file = File::create(directory.join("application.log"))?;
        Ok(Self {
            directory: directory.to_owned(),
            file: Some(file),
            bytes: 0,
        })
    }

    pub(super) fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.bytes + bytes.len() as u64 > MAX_FILE_BYTES {
            self.rotate()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic log is closed"))?;
        if let Err(error) = file.write_all(bytes) {
            self.bytes = file.metadata()?.len();
            return Err(error);
        }
        self.bytes += bytes.len() as u64;
        file.flush()
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.close()?;
        for index in (1..=BACKUPS).rev() {
            let source = self.directory.join(format!("application.{index}.log"));
            if index == BACKUPS {
                match fs::remove_file(source) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            } else {
                rename_if_present(
                    &source,
                    &self
                        .directory
                        .join(format!("application.{}.log", index + 1)),
                )?;
            }
        }
        fs::rename(
            self.directory.join("application.log"),
            self.directory.join("application.1.log"),
        )?;
        self.file = Some(File::create(self.directory.join("application.log"))?);
        self.bytes = 0;
        Ok(())
    }

    pub(super) fn close(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.take() {
            file.sync_all()?;
        }
        Ok(())
    }
}

fn rename_if_present(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(source, destination) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[derive(Clone)]
pub(super) struct LogWriter(pub(super) Arc<Mutex<RotatingFile>>);

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = EventWriter;

    fn make_writer(&'a self) -> Self::Writer {
        EventWriter {
            sink: self.clone(),
            bytes: Vec::new(),
            truncated: false,
        }
    }
}

pub(super) struct EventWriter {
    sink: LogWriter,
    bytes: Vec<u8>,
    truncated: bool,
}

impl Write for EventWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let available = (MAX_EVENT_BYTES - TRUNCATED.len()).saturating_sub(self.bytes.len());
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(available)]);
        self.truncated |= bytes.len() > available;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for EventWriter {
    fn drop(&mut self) {
        if self.truncated {
            if let Err(error) = std::str::from_utf8(&self.bytes) {
                self.bytes.truncate(error.valid_up_to());
            }
            self.bytes.extend_from_slice(TRUNCATED);
        }
        let result = self
            .sink
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .append(&self.bytes);
        if let Err(error) = result {
            eprintln!("ElegantClipboard diagnostic write failed: {error}");
        }
    }
}
