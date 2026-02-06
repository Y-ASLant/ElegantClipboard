pub mod clipboard;
pub mod file_ops;
pub mod settings;

use crate::clipboard::ClipboardMonitor;
use crate::database::Database;
use std::sync::Arc;

/// App state containing database and clipboard monitor
pub struct AppState {
    pub db: Database,
    pub monitor: ClipboardMonitor,
}

// ============ Shared Helpers ============

/// Hide the main window if it's not pinned, updating the window state accordingly.
pub(crate) fn hide_main_window_if_not_pinned(app: &tauri::AppHandle) {
    use tauri::Manager;

    if !crate::input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
        }
    }
}

/// Execute a closure with the clipboard monitor paused, then resume after a delay.
/// The monitor is paused immediately, and resumed 500ms later on a background task.
pub(crate) fn with_paused_monitor<F, R>(state: &Arc<AppState>, f: F) -> R
where
    F: FnOnce() -> R,
{
    state.monitor.pause();
    let result = f();

    let monitor = state.monitor.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        monitor.resume();
    });

    result
}

/// Open a path in the platform's native file explorer.
pub(crate) fn open_path_in_explorer(path: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}
