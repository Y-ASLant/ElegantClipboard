pub mod clipboard;
pub mod data_transfer;
pub mod file_ops;
pub mod groups;
pub mod preview;
pub mod settings;
pub mod sync;
pub mod translate;
pub mod window;
pub mod window_utils;

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

/// 多屏/高 DPI 下隐藏窗口后系统可能不自动还原前台窗口，导致 Ctrl+V 无接收者。
/// 仅在目标窗口不是当前前台窗口时才调用 SetForegroundWindow，
/// 避免冗余 WM_ACTIVATE 导致某些应用重置内部焦点/光标位置。
#[cfg(target_os = "windows")]
fn restore_prev_foreground_window() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    let prev = crate::input_monitor::get_prev_foreground_hwnd();
    if prev == 0 {
        tracing::warn!("hide: PREV_FOREGROUND_HWND is 0, cannot restore foreground window");
        return;
    }

    let hwnd = HWND(prev as *mut _);
    let current_fg = unsafe { GetForegroundWindow() };
    if current_fg.0 as isize == prev {
        tracing::info!("hide: target is already foreground, skipping SetForegroundWindow");
    } else if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        let _ = unsafe { SetForegroundWindow(hwnd) };
        tracing::info!("hide: restored foreground window hwnd={:#x}", prev);
    } else {
        tracing::warn!("hide: prev_hwnd={:#x} is no longer valid", prev);
    }
}

/// 隐藏主窗口或还原目标窗口焦点（用于粘贴前确保目标应用在前台）。
pub(crate) fn hide_main_window_if_not_pinned(app: &tauri::AppHandle) {
    if crate::input_monitor::is_window_pinned() {
        return;
    }
    let app = app.clone();
    if let Err(err) = crate::main_thread::run_on_ui_thread(&app.clone(), move || {
        hide_main_window_if_not_pinned_inner(&app)
    }) {
        tracing::warn!("hide_main_window_if_not_pinned dispatch failed: {err}");
    }
}

fn hide_main_window_if_not_pinned_inner(app: &tauri::AppHandle) {
    use tauri::Manager;

    let main_was_visible = match app.get_webview_window("main") {
        Some(window) if window.is_visible().unwrap_or(false) => {
            window::hide_main_window_inner(app, &window);
            true
        }
        _ => false,
    };

    hide_preview_windows_inner(app);

    #[cfg(target_os = "windows")]
    if main_was_visible {
        restore_prev_foreground_window();
    }
}

pub(crate) fn hide_preview_windows<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if crate::main_thread::is_main_thread() {
        hide_preview_windows_inner(app);
        return;
    }
    let app = app.clone();
    if let Err(err) =
        crate::main_thread::run_on_ui_thread(&app.clone(), move || hide_preview_windows_inner(&app))
    {
        tracing::warn!("hide_preview_windows dispatch failed: {err}");
    }
}

pub(crate) fn hide_preview_windows_inner<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    hide_image_preview_window(app);
    hide_text_preview_window(app);
}

pub(crate) fn hide_image_preview_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    preview::force_hide_image_preview(app);
}

pub(crate) fn hide_text_preview_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    preview::force_hide_text_preview(app);
}

/// 延迟恢复监控的发送端（全局单线程处理，避免每次粘贴都 spawn 新线程）
static RESUME_TX: std::sync::LazyLock<std::sync::mpsc::Sender<crate::clipboard::ClipboardMonitor>> =
    std::sync::LazyLock::new(|| {
        let (tx, rx) = std::sync::mpsc::channel::<crate::clipboard::ClipboardMonitor>();
        std::thread::Builder::new()
            .name("monitor-resume".into())
            .spawn(move || {
                loop {
                    let Ok(first) = rx.recv() else {
                        return;
                    };
                    let mut pending = vec![first];

                    // 防抖恢复请求：等待 500ms 静默期后批量处理
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
            .map_err(|e| format!("Failed to open folder: {e}"))?;
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
