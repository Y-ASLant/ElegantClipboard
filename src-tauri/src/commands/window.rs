use crate::commands::AppState;
use crate::database;
use tauri::{Emitter, Manager};

/// 若「记住窗口大小」开关启用，将当前窗口逻辑尺寸保存到 settings 表。
/// 所有隐藏主窗口的路径都应在 hide 前调用此函数。
pub(crate) fn save_window_size_if_enabled<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
) {
    if let Some(state) = app.try_state::<std::sync::Arc<AppState>>() {
        let settings_repo = database::SettingsRepository::new(&state.db);
        let persist = settings_repo
            .get("persist_window_size")
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true);
        if persist
            && let Ok(size) = window.inner_size()
            && let Ok(scale) = window.scale_factor()
        {
            let w = (size.width as f64 / scale).round() as u32;
            let h = (size.height as f64 / scale).round() as u32;
            let _ = settings_repo.set("window_width", &w.to_string());
            let _ = settings_repo.set("window_height", &h.to_string());
        }
    }
}

/// 切换主窗口显示/隐藏
pub(crate) fn toggle_window_visibility(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if crate::keyboard_hook::get_window_state() == crate::keyboard_hook::WindowState::Visible {
            save_window_size_if_enabled(app, &window);

            let _ = window.set_focusable(false);
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            crate::input_monitor::disable_mouse_monitoring();
            crate::commands::hide_preview_windows(app);
            let _ = window.emit("window-hidden", ());
        } else {
            let position_mode = app
                .try_state::<std::sync::Arc<AppState>>()
                .map(|state| {
                    let repo = database::SettingsRepository::new(&state.db);
                    let persist = repo
                        .get("persist_window_size")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true);
                    if persist {
                        let w = repo
                            .get("window_width")
                            .ok()
                            .flatten()
                            .and_then(|v| v.parse::<f64>().ok());
                        let h = repo
                            .get("window_height")
                            .ok()
                            .flatten()
                            .and_then(|v| v.parse::<f64>().ok());
                        if let (Some(w), Some(h)) = (w, h) {
                            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                                width: w,
                                height: h,
                            }));
                        }
                    }
                    // position_mode 优先；未设置时回退到旧版 follow_cursor
                    if let Some(mode_str) = repo.get("position_mode").ok().flatten() {
                        let mode = crate::positioning::PositionMode::from_str(&mode_str);
                        tracing::debug!("定位模式: {:?} (from position_mode='{}')", mode, mode_str);
                        mode
                    } else {
                        let follow = repo
                            .get("follow_cursor")
                            .ok()
                            .flatten()
                            .map(|v| v != "false")
                            .unwrap_or(true);
                        let mode = if follow {
                            crate::positioning::PositionMode::FollowCursor
                        } else {
                            crate::positioning::PositionMode::FixedPosition
                        };
                        tracing::debug!(
                            "定位模式: {:?} (legacy fallback, follow_cursor={})",
                            mode,
                            follow
                        );
                        mode
                    }
                })
                .unwrap_or(crate::positioning::PositionMode::FollowCursor);

            if let Err(e) = crate::positioning::position_window(&window, position_mode) {
                tracing::warn!("定位窗口失败: {}", e);
            }

            crate::input_monitor::save_current_focus();
            // 强制保持非激活展示，避免瞬态窗口（如 PowerToys/Wox 的 Alt+Enter 面板）因失焦关闭
            let _ = window.set_focusable(false);
            let _ = window.show();
            crate::positioning::force_topmost(&window);
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Visible);
            crate::input_monitor::enable_mouse_monitoring();
            let _ = window.emit("window-shown", ());
        }
    }
}

#[tauri::command]
pub async fn show_window(window: tauri::WebviewWindow) {
    let _ = window.show();
    crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Visible);
    let _ = window.emit("window-shown", ());
}

#[tauri::command]
pub async fn hide_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
    crate::input_monitor::disable_mouse_monitoring();
    crate::commands::hide_preview_windows(window.app_handle());
    let _ = window.emit("window-hidden", ());
}

#[tauri::command]
pub fn set_window_visibility(visible: bool) {
    crate::keyboard_hook::set_window_state(if visible {
        crate::keyboard_hook::WindowState::Visible
    } else {
        crate::keyboard_hook::WindowState::Hidden
    });
    if visible {
        crate::input_monitor::enable_mouse_monitoring();
    } else {
        crate::input_monitor::disable_mouse_monitoring();
    }
}

#[tauri::command]
pub async fn minimize_window(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
pub async fn toggle_maximize(window: tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
pub async fn close_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
    crate::input_monitor::disable_mouse_monitoring();
    crate::commands::hide_preview_windows(window.app_handle());
    let _ = window.emit("window-hidden", ());
}

#[tauri::command]
pub async fn set_window_pinned(window: tauri::WebviewWindow, pinned: bool) {
    crate::input_monitor::set_window_pinned(pinned);
    if pinned {
        let _ = window.set_focusable(false);
        #[cfg(windows)]
        {
            let prev = crate::input_monitor::get_prev_foreground_hwnd();
            if prev != 0 {
                unsafe {
                    let hwnd = windows::Win32::Foundation::HWND(prev as *mut _);
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
            }
        }
    }
}

#[tauri::command]
pub fn is_window_pinned() -> bool {
    crate::input_monitor::is_window_pinned()
}

#[tauri::command]
pub fn set_window_effect(
    window: tauri::WebviewWindow,
    effect: String,
    dark: Option<bool>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SWP_NOZORDER, SetWindowLongW, SetWindowPos, WS_EX_LAYERED,
        };

        let raw_hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let hwnd = HWND(raw_hwnd.0 as *mut _);

        let is_effect = effect != "none";

        unsafe {
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let has_layered = (ex_style as u32) & WS_EX_LAYERED.0 != 0;

            if is_effect && has_layered {
                SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    ((ex_style as u32) & !WS_EX_LAYERED.0) as i32,
                );
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            } else if !is_effect && !has_layered {
                SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    ((ex_style as u32) | WS_EX_LAYERED.0) as i32,
                );
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }

        let _ = window_vibrancy::clear_mica(&window);
        let _ = window_vibrancy::clear_acrylic(&window);
        let _ = window_vibrancy::clear_tabbed(&window);

        let apply_result: Result<(), String> = match effect.as_str() {
            "mica" => window_vibrancy::apply_mica(&window, dark)
                .map_err(|e| format!("Failed to apply mica: {}", e)),
            "acrylic" => window_vibrancy::apply_acrylic(&window, Some((0, 0, 0, 0)))
                .map_err(|e| format!("Failed to apply acrylic: {}", e)),
            "tabbed" => window_vibrancy::apply_tabbed(&window, dark)
                .map_err(|e| format!("Failed to apply tabbed: {}", e)),
            _ => Ok(()),
        };

        if let Err(ref e) = apply_result {
            tracing::warn!("Window effect '{}' not supported on this OS: {}", effect, e);
            // 恢复 WS_EX_LAYERED（应用失败时可能已被移除）
            unsafe {
                let cur_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                if (cur_style as u32) & WS_EX_LAYERED.0 == 0 {
                    SetWindowLongW(
                        hwnd,
                        GWL_EXSTYLE,
                        ((cur_style as u32) | WS_EX_LAYERED.0) as i32,
                    );
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                    );
                }
            }
        }

        apply_result?;

        tracing::info!("Window effect set to: {}", effect);
    }
    Ok(())
}

#[tauri::command]
pub async fn focus_clipboard_window(window: tauri::WebviewWindow) {
    crate::input_monitor::focus_clipboard_window(&window);
}

#[tauri::command]
pub async fn restore_last_focus(window: tauri::WebviewWindow) {
    crate::input_monitor::restore_last_focus(&window);
}

#[tauri::command]
pub fn save_current_focus() {
    crate::input_monitor::save_current_focus();
}

#[tauri::command]
pub async fn set_keyboard_nav_enabled(window: tauri::WebviewWindow, enabled: bool) {
    crate::input_monitor::set_keyboard_nav_enabled(enabled);
    // 不再因键盘导航切换而抢焦点，导航键通过低级钩子转发
    // 仅主窗口在关闭键盘导航时尝试还原焦点，避免设置窗口被意外切走
    let is_main_window = window.label() == "main";
    if is_main_window
        && !enabled
        && window.is_visible().unwrap_or(false)
        && !crate::input_monitor::is_window_pinned()
    {
        // 关闭时若窗口仍聚焦则恢复
        if window.is_focused().unwrap_or(false) {
            crate::input_monitor::restore_last_focus(&window);
        }
    }
}

#[tauri::command]
pub fn is_admin_launch_enabled() -> bool {
    crate::admin_launch::is_admin_launch_enabled()
}

#[tauri::command]
pub fn enable_admin_launch() -> Result<(), String> {
    crate::admin_launch::enable_admin_launch()
}

#[tauri::command]
pub fn disable_admin_launch() -> Result<(), String> {
    crate::admin_launch::disable_admin_launch()
}

#[tauri::command]
pub fn is_running_as_admin() -> bool {
    crate::admin_launch::is_running_as_admin()
}

#[tauri::command]
pub async fn check_for_update() -> Result<crate::updater::UpdateInfo, String> {
    tokio::task::spawn_blocking(crate::updater::check_update)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn download_update(
    app: tauri::AppHandle,
    download_url: String,
    file_name: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || crate::updater::download(&app, &download_url, &file_name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cancel_update_download() {
    crate::updater::cancel_download();
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle, installer_path: String) -> Result<(), String> {
    crate::updater::install(&installer_path)?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.exit(0);
    Ok(())
}
