use crate::database::SettingsRepository;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

use super::AppState;

// ============ Settings Commands ============

/// Get a setting value
#[tauri::command]
pub async fn get_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> Result<Option<String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get(&key).map_err(|e| e.to_string())
}

/// Set a setting value
#[tauri::command]
pub async fn set_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let repo = SettingsRepository::new(&state.db);
    repo.set(&key, &value).map_err(|e| e.to_string())
}

/// Get all settings
#[tauri::command]
pub async fn get_all_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<HashMap<String, String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get_all().map_err(|e| e.to_string())
}

// ============ Monitor Commands ============

/// Pause clipboard monitoring
#[tauri::command]
pub async fn pause_monitor(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.monitor.pause();
    Ok(())
}

/// Resume clipboard monitoring
#[tauri::command]
pub async fn resume_monitor(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.monitor.resume();
    Ok(())
}

/// Get monitor status
#[tauri::command]
pub async fn get_monitor_status(state: State<'_, Arc<AppState>>) -> Result<MonitorStatus, String> {
    Ok(MonitorStatus {
        running: state.monitor.is_running(),
        paused: state.monitor.is_paused(),
    })
}

#[derive(serde::Serialize)]
pub struct MonitorStatus {
    pub running: bool,
    pub paused: bool,
}

// ============ Database Commands ============

/// Optimize database
#[tauri::command]
pub async fn optimize_database(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.optimize().map_err(|e| e.to_string())?;
    Ok(())
}

/// Vacuum database
#[tauri::command]
pub async fn vacuum_database(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.vacuum().map_err(|e| e.to_string())?;
    Ok(())
}

// ============ Folder Commands ============

/// Open folder selection dialog (for settings window)
#[tauri::command]
pub async fn select_folder_for_settings(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let result = app
        .dialog()
        .file()
        .set_title("选择数据存储文件夹")
        .blocking_pick_folder();

    Ok(result.map(|p| p.to_string()))
}

/// Open data folder in file explorer
#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();
    super::open_path_in_explorer(&data_dir)
}

// ============ 自启动命令 ============
// 普通模式：tauri_plugin_autostart（注册表 Run 键）
// 管理员模式：任务计划程序（HIGHEST 运行级别）
// Windows 会静默跳过注册表 Run 中需要 UAC 提权的条目

/// 检查自启动是否启用（同时检查两种机制）
#[tauri::command]
pub async fn is_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    if crate::task_scheduler::is_autostart_task_exists() {
        return Ok(true);
    }
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 启用自启动（根据管理员模式选择机制）
#[tauri::command]
pub async fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    if crate::admin_launch::is_admin_launch_enabled() && crate::admin_launch::is_running_as_admin()
    {
        // 管理员模式：使用任务计划程序，清理注册表自启动
        crate::task_scheduler::create_autostart_task()?;
        let _ = app.autolaunch().disable();
        Ok(())
    } else {
        // 普通模式：使用注册表自启动，清理计划任务
        let _ = crate::task_scheduler::delete_autostart_task();
        app.autolaunch().enable().map_err(|e| e.to_string())
    }
}

/// 禁用自启动（同时清理两种机制）
#[tauri::command]
pub async fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    let _ = crate::task_scheduler::delete_autostart_task();
    use tauri_plugin_autostart::ManagerExt;
    let _ = app.autolaunch().disable();
    Ok(())
}

// ============ System Theme Commands ============

/// Convert RGB (0-255) to HSL string "H S% L%"
fn rgb_to_hsl_string(r: f64, g: f64, b: f64) -> String {
    let r_norm = r / 255.0;
    let g_norm = g / 255.0;
    let b_norm = b / 255.0;

    let max = r_norm.max(g_norm).max(b_norm);
    let min = r_norm.min(g_norm).min(b_norm);
    let delta = max - min;

    let mut h = 0.0;
    let mut s = 0.0;
    let l = (max + min) / 2.0;

    if delta > 0.0 {
        s = if l > 0.5 {
            delta / (2.0 - max - min)
        } else {
            delta / (max + min)
        };

        if max == r_norm {
            h = ((g_norm - b_norm) / delta).rem_euclid(6.0);
        } else if max == g_norm {
            h = (b_norm - r_norm) / delta + 2.0;
        } else {
            h = (r_norm - g_norm) / delta + 4.0;
        }
        h *= 60.0;
    }

    format!(
        "{} {}% {}%",
        h.round(),
        (s * 100.0).round(),
        (l * 100.0).round()
    )
}

/// Read the current accent color from registry (sync helper)
#[cfg(target_os = "windows")]
fn read_accent_color_from_registry() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let accent_key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent")
        .ok()?;
    let color_value: u32 = accent_key.get_value("AccentColorMenu").ok()?;
    // ABGR format
    let r = (color_value & 0xFF) as f64;
    let g = ((color_value >> 8) & 0xFF) as f64;
    let b = ((color_value >> 16) & 0xFF) as f64;
    Some(rgb_to_hsl_string(r, g, b))
}

/// Global app handle for the WndProc callback to emit events.
#[cfg(target_os = "windows")]
static WATCHER_APP_HANDLE: std::sync::OnceLock<parking_lot::Mutex<tauri::AppHandle>> =
    std::sync::OnceLock::new();

/// Start a background thread that listens for Windows `WM_SETTINGCHANGE` messages
/// with `"ImmersiveColorSet"` parameter (broadcast when accent color changes)
/// and emits `"system-accent-color-changed"` to the frontend.
#[cfg(target_os = "windows")]
pub fn start_accent_color_watcher(app_handle: tauri::AppHandle) {
    WATCHER_APP_HANDLE.get_or_init(|| parking_lot::Mutex::new(app_handle));

    std::thread::spawn(|| {
        use windows::Win32::Foundation::*;
        use windows::Win32::UI::WindowsAndMessaging::*;

        unsafe extern "system" fn wnd_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            unsafe {
                if msg == WM_SETTINGCHANGE {
                    let ptr = lparam.0 as *const u16;
                    if !ptr.is_null() {
                        // Read null-terminated wide string from lParam
                        let len = (0usize..256).find(|&i| *ptr.add(i) == 0).unwrap_or(0);
                        let slice = std::slice::from_raw_parts(ptr, len);
                        if slice == windows::core::w!("ImmersiveColorSet").as_wide() {
                            if let Some(handle) = WATCHER_APP_HANDLE.get() {
                                use tauri::Emitter;
                                let color = read_accent_color_from_registry();
                                let _ = handle.lock().emit("system-accent-color-changed", color);
                            }
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        unsafe {
            let class_name = windows::core::w!("ElegantClipboardAccentWatcher");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassW(&wc);

            // Create a hidden top-level window to receive broadcast messages.
            // NOTE: Do NOT use HWND_MESSAGE — message-only windows cannot receive
            // broadcast messages like WM_SETTINGCHANGE (HWND_BROADCAST skips them).
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                windows::core::w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            );

            // Run message loop — blocks until WM_QUIT
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

/// Get Windows system accent color in HSL format
#[tauri::command]
pub async fn get_system_accent_color() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            // Method 1: Registry AccentColorMenu — this is the actual user-chosen accent color
            if let Some(color) = read_accent_color_from_registry() {
                return Ok(Some(color));
            }

            // Method 2: Fallback to DwmGetColorizationColor
            // Note: This returns the DWM window frame blend color, which may differ
            // from the user's chosen accent color due to transparency blending.
            use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
            use windows_core::BOOL;

            let mut colorization: u32 = 0;
            let mut opaque_blend: BOOL = BOOL::from(false);

            if DwmGetColorizationColor(&mut colorization, &mut opaque_blend).is_ok() {
                let a = ((colorization >> 24) & 0xFF) as f64;
                let r = ((colorization >> 16) & 0xFF) as f64;
                let g = ((colorization >> 8) & 0xFF) as f64;
                let b = (colorization & 0xFF) as f64;

                if a > 10.0 {
                    return Ok(Some(rgb_to_hsl_string(r, g, b)));
                }
            }
        }

        Ok(None)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(None)
    }
}
