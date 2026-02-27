use crate::database::SettingsRepository;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

use super::AppState;

// ============ 设置命令 ============

/// 获取设置值
#[tauri::command]
pub async fn get_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> Result<Option<String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get(&key).map_err(|e| e.to_string())
}

/// 设置值
#[tauri::command]
pub async fn set_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let repo = SettingsRepository::new(&state.db);
    repo.set(&key, &value).map_err(|e| e.to_string())
}

/// 获取所有设置
#[tauri::command]
pub async fn get_all_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<HashMap<String, String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get_all().map_err(|e| e.to_string())
}

// ============ 监控命令 ============

/// 暂停剪贴板监控
#[tauri::command]
pub async fn pause_monitor(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.monitor.pause();
    tracing::info!("Clipboard monitor paused by user");
    Ok(())
}

/// 恢复剪贴板监控
#[tauri::command]
pub async fn resume_monitor(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.monitor.resume();
    tracing::info!("Clipboard monitor resumed by user");
    Ok(())
}

/// 获取监控状态
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

// ============ 数据库命令 ============

/// 优化数据库
#[tauri::command]
pub async fn optimize_database(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.optimize().map_err(|e| e.to_string())?;
    tracing::info!("Database optimized");
    Ok(())
}

/// 整理数据库
#[tauri::command]
pub async fn vacuum_database(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.db.vacuum().map_err(|e| e.to_string())?;
    tracing::info!("Database vacuumed");
    Ok(())
}

// ============ 文件夹命令 ============

/// 打开文件夹选择对话框
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

/// 在文件资源管理器中打开数据目录
#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();
    super::open_path_in_explorer(&data_dir)
}

// ============ 数据清理命令 ============

/// 重置所有设置为默认值（保留剪贴板数据）
#[tauri::command]
pub async fn reset_settings(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let repo = SettingsRepository::new(&state.db);
    repo.clear_all().map_err(|e| e.to_string())?;
    tracing::info!("All settings reset to defaults");
    Ok(())
}

/// 重置所有数据（删除剪贴板条目 + 设置 + 图片文件）
#[tauri::command]
pub async fn reset_all_data(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    use crate::database::ClipboardRepository;
    use std::fs;
    use tracing::info;

    // 清空剪贴板数据
    let clipboard_repo = ClipboardRepository::new(&state.db);
    let image_paths = clipboard_repo.get_all_image_paths().unwrap_or_default();
    clipboard_repo.clear_all().map_err(|e| e.to_string())?;
    crate::clipboard::cleanup_image_files(&image_paths);

    // 清空设置
    let settings_repo = SettingsRepository::new(&state.db);
    settings_repo.clear_all().map_err(|e| e.to_string())?;

    // 删除图片/图标目录（清理残留文件）
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();
    for dir_name in &["images", "icons"] {
        let dir = data_dir.join(dir_name);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    state.db.vacuum().ok();

    info!("Reset all data completed");
    Ok(())
}

// ============ 自启动命令 ============
// 始终使用 tauri_plugin_autostart（注册表 Run 键）。
// 管理员模式下应用会在启动后自行提权，无需单独的自启动机制。

/// 检查自启动是否启用
#[tauri::command]
pub async fn is_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// 启用自启动
#[tauri::command]
pub async fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().enable().map_err(|e| e.to_string())
}

/// 禁用自启动
#[tauri::command]
pub async fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().disable().map_err(|e| e.to_string())
}

// ============ 系统主题命令 ============

/// RGB (0-255) 转 HSL 字符串 "H S% L%"
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

/// 从注册表读取当前强调色
#[cfg(target_os = "windows")]
fn read_accent_color_from_registry() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let accent_key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent")
        .ok()?;
    let color_value: u32 = accent_key.get_value("AccentColorMenu").ok()?;
    // ABGR 格式
    let r = (color_value & 0xFF) as f64;
    let g = ((color_value >> 8) & 0xFF) as f64;
    let b = ((color_value >> 16) & 0xFF) as f64;
    Some(rgb_to_hsl_string(r, g, b))
}

/// WndProc 回调用的全局 AppHandle
#[cfg(target_os = "windows")]
static WATCHER_APP_HANDLE: std::sync::OnceLock<parking_lot::Mutex<tauri::AppHandle>> =
    std::sync::OnceLock::new();

/// 启动后台线程监听 `WM_SETTINGCHANGE` 中的 `"ImmersiveColorSet"` 广播
/// 当系统强调色变化时向前端发射 `"system-accent-color-changed"` 事件
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
                    // 读取 lParam 中的 null 结尾宽字符串
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

            // 创建隐藏顶级窗口接收广播消息
            // 注意：不能用 HWND_MESSAGE，纯消息窗口无法收到 WM_SETTINGCHANGE 广播
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

            // 消息循环（阻塞直到 WM_QUIT）
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

/// 获取 Windows 系统强调色（HSL 格式）
#[tauri::command]
pub async fn get_system_accent_color() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            // 方式一：注册表 AccentColorMenu（用户实际选择的强调色）
            if let Some(color) = read_accent_color_from_registry() {
                return Ok(Some(color));
            }

            // 方式二：回退到 DwmGetColorizationColor
            // 注意：此值为 DWM 窗口边框混合色，因透明度混合可能与用户选择的强调色不同
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
