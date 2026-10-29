use super::*;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tracing_subscriber::fmt::MakeWriter;

fn session_directories(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    paths
}

#[test]
fn rotation_caps_files_and_retains_newest_events() {
    let root = tempfile::tempdir().unwrap();
    let sink = LogWriter(Arc::new(Mutex::new(
        RotatingFile::open(root.path()).unwrap(),
    )));
    let payload = vec![b'x'; MAX_EVENT_BYTES * 2];
    for sequence in 0..24 {
        let mut writer = sink.make_writer();
        writeln!(writer, "event-{sequence:02}").unwrap();
        writer.write_all(&payload).unwrap();
    }
    sink.0.lock().unwrap().close().unwrap();
    let files: Vec<_> = fs::read_dir(root.path()).unwrap().collect();
    assert_eq!(files.len(), 4);
    for entry in files {
        assert!(entry.unwrap().metadata().unwrap().len() <= 4 * 1024 * 1024);
    }
    let latest = fs::read_to_string(root.path().join("application.log")).unwrap();
    assert!(latest.contains("event-23"));
    assert!(!latest.contains("event-00"));
    assert!(latest.contains("event exceeded 1 MiB"));
}

#[test]
fn retention_keeps_active_sessions_and_only_five_newest_inactive() {
    let root = tempfile::tempdir().unwrap();
    let active = Session::create(root.path()).unwrap();
    let mut completed = Vec::new();
    for _ in 0..9 {
        let session = Session::create(root.path()).unwrap();
        completed.push(session.directory.clone());
        drop(session);
    }
    assert!(active.directory.exists());
    let retained = session_directories(root.path());
    assert_eq!(retained.len(), 6);
    assert!(completed[..4].iter().all(|path| !path.exists()));
    assert!(completed[4..].iter().all(|path| path.exists()));
    drop(active);
    assert_eq!(session_directories(root.path()).len(), 5);
}

#[test]
fn retention_preserves_unrelated_directories_and_files() {
    let root = tempfile::tempdir().unwrap();
    let directories = [
        "session-notes",
        "session-123-456-0",
        "session-000000000000000000000000000001-pid-0",
        "session-000000000000000000000000000001-456-",
        "session-000000000000000000000000000001-456-0-extra",
    ];
    for name in directories {
        let directory = root.path().join(name);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("notes.txt"), b"unrelated notes").unwrap();
    }
    let unrelated_file = root
        .path()
        .join("session-000000000000000000000000000001-456-0");
    fs::write(&unrelated_file, b"unrelated file").unwrap();
    for _ in 0..7 {
        drop(Session::create(root.path()).unwrap());
    }
    for name in directories {
        assert_eq!(
            fs::read(root.path().join(name).join("notes.txt")).unwrap(),
            b"unrelated notes"
        );
    }
    assert_eq!(fs::read(unrelated_file).unwrap(), b"unrelated file");
}

#[test]
fn concurrent_event_writers_keep_records_intact() {
    let root = tempfile::tempdir().unwrap();
    let sink = LogWriter(Arc::new(Mutex::new(
        RotatingFile::open(root.path()).unwrap(),
    )));
    std::thread::scope(|scope| {
        for thread in 0..8 {
            let sink = &sink;
            scope.spawn(move || {
                for record in 0..100 {
                    writeln!(sink.make_writer(), "thread-{thread}-record-{record}").unwrap();
                }
            });
        }
    });
    sink.0.lock().unwrap().close().unwrap();
    let text = fs::read_to_string(root.path().join("application.log")).unwrap();
    let lines: std::collections::HashSet<_> = text.lines().collect();
    assert_eq!(lines.len(), 800);
    for thread in 0..8 {
        for record in 0..100 {
            assert!(lines.contains(format!("thread-{thread}-record-{record}").as_str()));
        }
    }
}

fn child(root: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "diagnostics::tests::subprocess_entry",
            "--nocapture",
        ])
        .env("ELEGANTCLIPBOARD_DIAGNOSTICS_TEST_ROOT", root)
        .env("ELEGANTCLIPBOARD_DIAGNOSTICS_TEST_MODE", mode)
        .env_remove("RUST_LOG")
        .env_remove("RUST_BACKTRACE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

#[test]
fn concurrent_processes_have_separate_logs_and_default_privacy_filter() {
    let root = tempfile::tempdir().unwrap();
    let first = child(root.path(), "write").spawn().unwrap();
    let second = child(root.path(), "write").spawn().unwrap();
    assert!(first.wait_with_output().unwrap().status.success());
    assert!(second.wait_with_output().unwrap().status.success());
    let sessions = session_directories(root.path());
    assert_eq!(sessions.len(), 2);
    for session in sessions {
        let text = fs::read_to_string(session.join("application.log")).unwrap();
        assert!(text.contains("application-debug-marker"));
        assert!(text.contains("dependency-warning-marker"));
        assert!(!text.contains("private-group-marker"));
        assert!(!text.contains("dependency-debug-marker"));
        assert_eq!(text.matches("concurrent-record").count(), 100);
    }
}

#[test]
fn panic_subprocess_persists_backtrace_with_busy_writer_before_unwind_and_abort() {
    for mode in ["panic", "abort"] {
        let root = tempfile::tempdir().unwrap();
        let output = child(root.path(), mode).output().unwrap();
        assert!(!output.status.success());
        let sessions = session_directories(root.path());
        assert_eq!(sessions.len(), 1);
        let panic = fs::read_to_string(sessions[0].join("panic.log")).unwrap();
        assert!(panic.contains("isolated-diagnostic-panic"));
        assert!(panic.contains("diagnostics/tests.rs") || panic.contains("diagnostics\\tests.rs"));
        assert!(panic.contains("thread="));
        assert!(panic.contains("Forced backtrace:"));
        assert!(
            panic
                .lines()
                .any(|line| line.trim_start().starts_with("0:"))
        );
        assert!(panic.len() <= MAX_EVENT_BYTES);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("isolated-diagnostic-panic"));
        assert!(stderr.contains("panicked at"));
        // An abnormal exit releases the OS lease, so future sessions can prune it.
        for _ in 0..6 {
            drop(Session::create(root.path()).unwrap());
        }
        assert!(!sessions[0].exists());
    }
}

// Invoked only by this test executable; no production crash switch exists.
#[test]
fn subprocess_entry() {
    let Some(root) = std::env::var_os("ELEGANTCLIPBOARD_DIAGNOSTICS_TEST_ROOT") else {
        return;
    };
    let mode = std::env::var("ELEGANTCLIPBOARD_DIAGNOSTICS_TEST_MODE").unwrap();
    let directory = Arc::new(OnceLock::new());
    install_panic_hook(directory.clone());
    let diagnostics = start_session(Path::new(&root), &directory).unwrap();
    if mode == "write" {
        tracing::debug!("application-debug-marker");
        tracing::warn!(target: "dependency", "dependency-warning-marker");
        tracing::debug!(target: "dependency", "dependency-debug-marker");
        tracing::debug!(target: "clipboard_core::database::groups", "private-group-marker");
        for record in 0..100 {
            tracing::debug!(record, "concurrent-record");
        }
        return;
    }
    if mode == "abort" {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            hook(info);
            std::process::abort();
        }));
    }
    let _busy_writer = diagnostics.writer.0.lock().unwrap();
    panic!("isolated-diagnostic-panic");
}
