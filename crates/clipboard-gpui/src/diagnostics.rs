mod storage;
#[cfg(test)]
mod tests;

use std::backtrace::Backtrace;
use std::fmt::{self, Write as _};
use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use directories::BaseDirs;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::prelude::*;

use storage::{LogWriter, MAX_EVENT_BYTES, RotatingFile, Session};

pub(crate) struct Diagnostics {
    writer: LogWriter,
    // Drop the session lease only after closing the writer.
    session: Session,
}

pub(crate) fn initialize() -> Option<Diagnostics> {
    let panic_directory = Arc::new(OnceLock::new());
    install_panic_hook(panic_directory.clone());
    let result = (|| -> anyhow::Result<Diagnostics> {
        let base = BaseDirs::new()
            .ok_or_else(|| io::Error::other("local application data directory is unavailable"))?;
        start_session(
            &base.data_local_dir().join("ElegantClipboardLogs"),
            &panic_directory,
        )
    })();
    match result {
        Ok(diagnostics) => {
            tracing::info!(
                session = %diagnostics.session.directory.display(),
                "local diagnostics enabled"
            );
            match std::env::current_exe().and_then(std::fs::metadata) {
                Ok(metadata) => tracing::info!(
                    executable_bytes = metadata.len(),
                    executable_modified = ?metadata.modified().ok(),
                    "executable build identity"
                ),
                Err(error) => tracing::warn!(%error, "could not read executable build identity"),
            }
            Some(diagnostics)
        }
        Err(error) => {
            eprintln!("ElegantClipboard diagnostics initialization failed: {error:#}");
            None
        }
    }
}

fn start_session(root: &Path, panic_directory: &OnceLock<PathBuf>) -> anyhow::Result<Diagnostics> {
    let session = Session::create(root)?;
    let _ = panic_directory.set(session.directory.clone());
    let writer = LogWriter(Arc::new(Mutex::new(RotatingFile::open(
        &session.directory,
    )?)));
    let filter = Targets::new()
        .with_default(LevelFilter::WARN)
        .with_target("elegant_clipboard_gpui", LevelFilter::DEBUG)
        .with_target("clipboard_platform", LevelFilter::DEBUG)
        .with_target("clipboard_core", LevelFilter::DEBUG)
        // Existing repository DEBUG messages contain user-supplied group names.
        .with_target("clipboard_core::database::groups", LevelFilter::INFO);
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(writer.clone()),
        )
        .try_init()
        .map_err(|error| anyhow::anyhow!("installing diagnostic subscriber: {error}"))?;
    Ok(Diagnostics { writer, session })
}

impl Drop for Diagnostics {
    fn drop(&mut self) {
        tracing::info!(
            session = %self.session.directory.display(),
            panicking = std::thread::panicking(),
            "local diagnostics closing"
        );
        if let Err(error) = self
            .writer
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .close()
        {
            eprintln!("ElegantClipboard diagnostic flush failed: {error}");
        }
    }
}

fn install_panic_hook(directory: Arc<OnceLock<PathBuf>>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Never enter the subscriber or its mutex from a panic hook. The first
        // panic owns a separate file, even if the normal writer is busy/poisoned.
        let report = panic_report(info);
        if let Some(directory) = directory.get()
            && let Err(error) = persist_panic(directory, &report)
        {
            eprintln!("ElegantClipboard panic log write failed: {error}");
        }
        let _ = io::stderr().write_all(report.as_bytes());
        previous(info);
    }));
}

fn panic_report(info: &PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    let mut report = CappedText {
        text: String::new(),
        limit: 64 * 1024,
        truncated: false,
    };
    let _ = writeln!(
        report,
        "{} PANIC pid={} thread={:?} name={:?}\n{info}",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        std::process::id(),
        thread.id(),
        thread.name(),
    );
    if report.truncated {
        report
            .text
            .push_str("\n[panic message exceeded 64 KiB; remainder omitted]\n");
    }
    report.limit = MAX_EVENT_BYTES - 128;
    report.truncated = false;
    let _ = writeln!(
        report,
        "\nForced backtrace:\n{}",
        Backtrace::force_capture()
    );
    if report.truncated {
        report
            .text
            .push_str("\n[panic report exceeded 1 MiB; remainder omitted]\n");
    }
    report.text
}

fn persist_panic(directory: &Path, report: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("panic.log"))?;
    file.write_all(report.as_bytes())?;
    file.flush()?;
    file.sync_all()
}

struct CappedText {
    text: String,
    limit: usize,
    truncated: bool,
}

impl fmt::Write for CappedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut length = value.len().min(self.limit.saturating_sub(self.text.len()));
        while !value.is_char_boundary(length) {
            length -= 1;
        }
        self.text.push_str(&value[..length]);
        self.truncated |= length < value.len();
        Ok(())
    }
}
