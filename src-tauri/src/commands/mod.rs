pub mod clipboard;
pub mod file_ops;
pub mod settings;

use crate::clipboard::ClipboardMonitor;
use crate::database::Database;
use std::sync::Arc;

/// 应用状态：包含数据库与剪贴板监控器
pub struct AppState {
    pub db: Database,
    pub monitor: ClipboardMonitor,
}

// ============ 公用辅助函数 ============

/// 非固定状态下隐藏主窗口并更新状态，同时隐藏图片预览窗口。
pub(crate) fn hide_main_window_if_not_pinned(app: &tauri::AppHandle) {
    use tauri::Manager;

    if !crate::input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
        }
        // 隐藏图片预览窗口（主窗口消失时无法触发 onMouseLeave）
        hide_image_preview_window(app);
    }
}

/// 隐藏图片预览窗口（若存在）。
/// 主窗口隐藏时调用，防止预览残留（onMouseLeave 不会触发）。
pub(crate) fn hide_image_preview_window(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    if let Some(preview) = app.get_webview_window("image-preview") {
        let _ = preview.hide();
        let _ = preview.emit("image-preview-clear", ());
    }
}

/// 暂停剪贴板监控并执行闭包，500ms 后在后台线程恢复监控。
pub(crate) fn with_paused_monitor<F, R>(state: &Arc<AppState>, f: F) -> R
where
    F: FnOnce() -> R,
{
    state.monitor.pause();
    let result = f();

    let monitor = state.monitor.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        monitor.resume();
    });

    result
}

/// 用系统文件管理器打开指定路径。
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
