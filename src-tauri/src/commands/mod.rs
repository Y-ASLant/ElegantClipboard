pub mod clipboard;
pub mod file_ops;
pub mod groups;
pub mod settings;

use crate::clipboard::ClipboardMonitor;
use crate::database::Database;
use parking_lot::Mutex;
use std::sync::Arc;

/// 应用状态：包含数据库与剪贴板监控器
pub struct AppState {
    pub db: Database,
    pub monitor: ClipboardMonitor,
    /// 当前活动分组 ID（None = 默认分组）
    pub active_group_id: Arc<Mutex<Option<i64>>>,
}

/// 隐藏主窗口或还原目标窗口焦点（用于粘贴前确保目标应用在前台）。
pub(crate) fn hide_main_window_if_not_pinned(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    if !crate::input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            // 窗口已隐藏时无需操作（快捷粘贴 Alt+N 不经过 UI，窗口本就不可见）
            if !window.is_visible().unwrap_or(false) {
                return;
            }
            crate::save_window_size_if_enabled(app, &window);
            let _ = window.set_focusable(false);
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            crate::input_monitor::disable_mouse_monitoring();
            let _ = window.emit("window-hidden", ());
        }
        hide_preview_windows(app);

        // 多屏/高 DPI 下隐藏窗口后系统可能不自动还原前台窗口，导致 Ctrl+V 无接收者。
        // 仅在目标窗口不是当前前台窗口时才调用 SetForegroundWindow，
        // 避免冗余 WM_ACTIVATE 导致某些应用重置内部焦点/光标位置。
        #[cfg(target_os = "windows")]
        {
            let prev = crate::input_monitor::get_prev_foreground_hwnd();
            if prev != 0 {
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetForegroundWindow, SetForegroundWindow, IsWindow,
                };
                use windows::Win32::Foundation::HWND;
                let hwnd = HWND(prev as *mut _);
                let current_fg = unsafe { GetForegroundWindow() };
                if current_fg.0 as isize == prev {
                    tracing::info!("hide: 目标窗口已是前台，跳过 SetForegroundWindow");
                } else if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
                    let _ = unsafe { SetForegroundWindow(hwnd) };
                    tracing::info!("hide: 已恢复前台窗口 hwnd={:#x}", prev);
                } else {
                    tracing::warn!("hide: prev_hwnd={:#x} 已无效", prev);
                }
            } else {
                tracing::warn!("hide: PREV_FOREGROUND_HWND 为 0，无法恢复前台窗口");
            }
        }
    }
}

/// 隐藏图片预览窗口（若存在）。
pub(crate) fn hide_image_preview_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::{Emitter, Manager};

    if let Some(preview) = app.get_webview_window("image-preview") {
        let _ = preview.hide();
        let _ = preview.emit("image-preview-clear", ());
    }
}

/// 隐藏文本预览窗口（若存在）。
pub(crate) fn hide_text_preview_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::{Emitter, Manager};

    if let Some(preview) = app.get_webview_window("text-preview") {
        let _ = preview.hide();
        let _ = preview.emit("text-preview-clear", ());
    }
}

/// 隐藏所有悬浮预览窗口（图片 / 文本）。
pub(crate) fn hide_preview_windows<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    hide_image_preview_window(app);
    hide_text_preview_window(app);
}

/// 延迟恢复监控的发送端（全局单线程处理，避免每次粘贴都 spawn 新线程）
static RESUME_TX: std::sync::LazyLock<std::sync::mpsc::Sender<crate::clipboard::ClipboardMonitor>> =
    std::sync::LazyLock::new(|| {
        let (tx, rx) = std::sync::mpsc::channel::<crate::clipboard::ClipboardMonitor>();
        std::thread::Builder::new()
            .name("monitor-resume".into())
            .spawn(move || {
                loop {
                    let first = match rx.recv() {
                        Ok(monitor) => monitor,
                        Err(_) => return,
                    };
                    let mut pending = vec![first];

                    // Debounce resume requests: wait for a quiet 500ms window,
                    // then flush all pending resumes together.
                    loop {
                        match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                            Ok(monitor) => pending.push(monitor),
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                for monitor in pending.drain(..) {
                                    monitor.resume();
                                }
                                break;
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                for monitor in pending.drain(..) {
                                    monitor.resume();
                                }
                                return;
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn monitor-resume thread");
        tx
    });

/// 暂停剪贴板监控并执行闭包，500ms 后恢复监控。
pub(crate) fn with_paused_monitor<F, R>(state: &Arc<AppState>, f: F) -> R
where
    F: FnOnce() -> R,
{
    state.monitor.pause();
    let result = f();

    let _ = RESUME_TX.send(state.monitor.clone());

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
