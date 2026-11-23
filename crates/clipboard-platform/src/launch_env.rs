//! Internal launch switches for the GUI binary (no CLI).
//! Set by autostart entries, elevation relaunch, import preview, and CI smoke.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const DATA_DIR: &str = "ELEGANTCLIPBOARD_DATA_DIR";
pub const START_HIDDEN: &str = "ELEGANTCLIPBOARD_START_HIDDEN";
pub const NO_MONITOR: &str = "ELEGANTCLIPBOARD_NO_MONITOR";
pub const SMOKE_TEST: &str = "ELEGANTCLIPBOARD_SMOKE_TEST";

const WAIT_FLAG: &str = ".wait-for-instance";

pub fn truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        ),
        Err(_) => false,
    }
}

pub fn data_dir() -> Option<PathBuf> {
    env::var_os(DATA_DIR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn start_hidden() -> bool {
    truthy(START_HIDDEN)
}

pub fn monitor() -> bool {
    !truthy(NO_MONITOR) && !truthy(SMOKE_TEST)
}

pub fn smoke_test() -> bool {
    truthy(SMOKE_TEST)
}

/// Mark the next process using this data directory to wait for the instance lock.
pub fn mark_wait_for_instance(data_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(data_dir)?;
    fs::write(data_dir.join(WAIT_FLAG), b"")
}

/// Consume a one-shot wait flag left by elevation relaunch.
pub fn take_wait_for_instance(data_dir: &Path) -> bool {
    let path = data_dir.join(WAIT_FLAG);
    if !path.is_file() {
        return false;
    }
    let _ = fs::remove_file(&path);
    true
}

/// Apply launch env on a child process. `None` clears DATA_DIR so the child
/// reads `data-location` after a pending relocate (must not inherit the old path).
pub fn apply(command: &mut std::process::Command, data_dir: Option<&Path>) {
    match data_dir {
        Some(path) => {
            command.env(DATA_DIR, path);
        }
        None => {
            command.env_remove(DATA_DIR);
        }
    }
}

pub fn apply_no_monitor(command: &mut std::process::Command) {
    command.env(NO_MONITOR, "1");
}

pub fn apply_start_hidden(command: &mut std::process::Command) {
    command.env(START_HIDDEN, "1");
}

pub fn apply_smoke_test(command: &mut std::process::Command) {
    command.env(SMOKE_TEST, "1");
    command.env(NO_MONITOR, "1");
}
