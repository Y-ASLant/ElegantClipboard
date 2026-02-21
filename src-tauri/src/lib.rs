mod admin_launch;
mod clipboard;
mod commands;
mod config;
mod database;
mod input_monitor;
mod keyboard_hook;
mod positioning;
mod shortcut;
mod task_scheduler;
mod tray;
mod updater;
mod win_v_registry;

use clipboard::ClipboardMonitor;
use commands::AppState;
use config::AppConfig;
use database::Database;
use database::SettingsRepository;
use shortcut::parse_shortcut;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::Level;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Global state for current shortcut (parking_lot::RwLock: no poison, consistent with codebase)
static CURRENT_SHORTCUT: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);
/// In-memory cache for quick paste shortcuts (slot 1-9)
static CURRENT_QUICK_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> =
    parking_lot::RwLock::new(Vec::new());
/// Serialise quick-paste operations so concurrent shortcut events
/// don't race on the system clipboard.
static QUICK_PASTE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
/// Track which quick-paste slots are "active" (key held down).
/// First press → full pipeline; repeat press → simulate_paste only.
static ACTIVE_QUICK_PASTE_SLOTS: std::sync::LazyLock<parking_lot::Mutex<HashSet<u8>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashSet::new()));

fn default_quick_paste_shortcuts() -> Vec<String> {
    (1..=9).map(|slot| format!("Alt+{}", slot)).collect()
}

fn quick_paste_setting_key(slot: u8) -> String {
    format!("quick_paste_shortcut_{}", slot)
}

fn normalize_shortcut_value(value: &str) -> String {
    value.trim().to_string()
}

fn shortcut_has_modifier(shortcut: &str) -> bool {
    // Shift alone is not a valid modifier for global shortcuts (e.g. Shift+1 just types '!')
    shortcut
        .split('+')
        .map(|part| part.trim().to_uppercase())
        .any(|part| matches!(part.as_str(), "CTRL" | "CONTROL" | "ALT" | "WIN" | "SUPER" | "META" | "CMD"))
}

fn load_quick_paste_shortcuts(repo: &SettingsRepository) -> Vec<String> {
    let mut shortcuts = default_quick_paste_shortcuts();
    for slot in 1..=9 {
        let key = quick_paste_setting_key(slot);
        if let Ok(Some(value)) = repo.get(&key) {
            shortcuts[(slot - 1) as usize] = normalize_shortcut_value(&value);
        }
    }
    shortcuts
}

fn apply_quick_paste_shortcuts(
    app: &tauri::AppHandle,
    shortcuts: &[String],
) -> HashMap<u8, String> {
    // Unregister previously active quick paste shortcuts first
    // (values are already normalized when stored)
    for s in CURRENT_QUICK_PASTE_SHORTCUTS.read().iter() {
        if s.is_empty() {
            continue;
        }
        if let Some(shortcut) = parse_shortcut(s) {
            let _ = app.global_shortcut().unregister(shortcut);
        }
    }

    let mut failures = HashMap::new();
    let mut applied = vec![String::new(); 9];

    for slot in 1..=9 {
        let idx = (slot - 1) as usize;
        let shortcut_str = shortcuts.get(idx).cloned().unwrap_or_default();
        let normalized = normalize_shortcut_value(&shortcut_str);
        applied[idx] = normalized.clone();

        if normalized.is_empty() {
            continue;
        }

        let parsed = match parse_shortcut(&normalized) {
            Some(v) => v,
            None => {
                failures.insert(slot, format!("槽位 {} 快捷键格式无效: {}", slot, normalized));
                continue;
            }
        };

        let reg_result = app
            .global_shortcut()
            .on_shortcut(parsed, move |app, _shortcut, event| match event.state {
                ShortcutState::Pressed => {
                    // Skip when one of our own windows (settings, editor, …)
                    // has focus — otherwise simulate_paste sends Ctrl+V into
                    // our own UI.
                    let any_focused = app
                        .webview_windows()
                        .values()
                        .any(|w| w.is_focused().unwrap_or(false));
                    if any_focused {
                        return;
                    }

                    // Distinguish first press vs repeat press (key held).
                    let is_first = {
                        let mut active = ACTIVE_QUICK_PASTE_SLOTS.lock();
                        active.insert(slot) // true = newly inserted
                    };

                    let state = app.state::<Arc<AppState>>().inner().clone();
                    let app_handle = app.clone();
                    std::thread::spawn(move || {
                        // Serialise so concurrent events don't race on the
                        // system clipboard.
                        let _guard = QUICK_PASTE_LOCK.lock();

                        if is_first {
                            // Full pipeline: read DB → clipboard → paste.
                            if let Err(err) = commands::clipboard::quick_paste_by_slot(
                                &state, &app_handle, slot,
                            ) {
                                tracing::warn!("Quick paste slot {} failed: {}", slot, err);
                                ACTIVE_QUICK_PASTE_SLOTS.lock().remove(&slot);
                            }
                        } else {
                            // Repeat press: content is already in the clipboard,
                            // just simulate Ctrl+V again.
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            if let Err(err) = commands::clipboard::simulate_paste() {
                                tracing::warn!("Quick paste repeat slot {} failed: {}", slot, err);
                            }
                        }
                    });
                }
                ShortcutState::Released => {
                    ACTIVE_QUICK_PASTE_SLOTS.lock().remove(&slot);
                }
            });

        if let Err(err) = reg_result {
            failures.insert(
                slot,
                format!("槽位 {} 注册失败（{}）: {}", slot, normalized, err),
            );
        }
    }

    *CURRENT_QUICK_PASTE_SHORTCUTS.write() = applied;
    failures
}

/// Keep the non-blocking writer guard alive for the entire process.
/// Dropping it would flush and stop the background writer thread.
static FILE_LOG_GUARD: parking_lot::Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
    parking_lot::Mutex::new(None);

/// Rotate the log file if it exceeds the size limit.
/// Renames `app.log` → `app.log.old` (overwriting any previous backup).
fn rotate_log_if_needed(log_path: &std::path::Path, max_size: u64) {
    if let Ok(meta) = std::fs::metadata(log_path) {
        if meta.len() > max_size {
            let backup = log_path.with_extension("log.old");
            let _ = std::fs::rename(log_path, backup);
        }
    }
}

/// Initialize logging system.
/// When `config.log_to_file` is true, an additional file layer writes to `app.log`
/// in the data directory.  The file is rotated when it exceeds 10 MB.
fn init_logging(config: &AppConfig) {
    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    let file_layer = if config.is_log_to_file() {
        let log_path = config.get_log_path();
        // Ensure the parent directory exists
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        rotate_log_if_needed(&log_path, config::DEFAULT_LOG_MAX_SIZE);

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                let (non_blocking, guard) = tracing_appender::non_blocking(file);
                *FILE_LOG_GUARD.lock() = Some(guard);
                Some(
                    fmt::layer()
                        .with_target(false)
                        .with_thread_ids(false)
                        .with_file(true)
                        .with_line_number(true)
                        .with_ansi(false)
                        .with_writer(non_blocking),
                )
            }
            Err(e) => {
                eprintln!("Failed to open log file {}: {e}", log_path.display());
                None
            }
        }
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(Level::INFO))
        .with(stdout_layer)
        .with(file_layer)
        .init();
}

/// Tauri command: Get app version
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Tauri command: Show main window
#[tauri::command]
async fn show_window(window: tauri::WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
    // Emit event to frontend for cache invalidation
    let _ = window.emit("window-shown", ());
}

/// Tauri command: Hide main window
#[tauri::command]
async fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
    // Hide image preview window
    commands::hide_image_preview_window(window.app_handle());
    // Emit event to frontend so it can reset state while hidden
    let _ = window.emit("window-hidden", ());
}

/// Tauri command: Set window visibility state (for sync with backend)
#[tauri::command]
fn set_window_visibility(visible: bool) {
    keyboard_hook::set_window_state(if visible {
        keyboard_hook::WindowState::Visible
    } else {
        keyboard_hook::WindowState::Hidden
    });
    // Also enable/disable mouse monitoring for click-outside detection
    if visible {
        input_monitor::enable_mouse_monitoring();
    } else {
        input_monitor::disable_mouse_monitoring();
    }
}

/// Tauri command: Minimize window
#[tauri::command]
async fn minimize_window(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

/// Tauri command: Toggle maximize window
#[tauri::command]
async fn toggle_maximize(window: tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

/// Tauri command: Close window (hide to tray)
#[tauri::command]
async fn close_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
    // Hide image preview window
    commands::hide_image_preview_window(window.app_handle());
}

/// Tauri command: Get default data path (returns current configured path)
#[tauri::command]
fn get_default_data_path() -> String {
    let config = AppConfig::load();
    config.get_data_dir().to_string_lossy().to_string()
}

/// Tauri command: Get the original default data path (not from config)
#[tauri::command]
fn get_original_default_path() -> String {
    database::get_default_db_path()
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Tauri command: Set data path and save to config
#[tauri::command]
fn set_data_path(path: String) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.data_path = if path.is_empty() { None } else { Some(path) };
    config.save()
}

/// Tauri command: Migrate data to new path
#[tauri::command]
fn migrate_data_to_path(new_path: String) -> Result<config::MigrationResult, String> {
    let config = AppConfig::load();
    let old_path = config.get_data_dir();
    let new_path = std::path::PathBuf::from(&new_path);

    // Don't migrate if paths are the same
    if old_path == new_path {
        return Err("Source and destination paths are the same".to_string());
    }

    // Perform migration
    let result = config::migrate_data(&old_path, &new_path)?;

    // If migration successful, update config
    if result.success() {
        let mut new_config = AppConfig::load();
        new_config.data_path = Some(new_path.to_string_lossy().to_string());
        new_config.save()?;
    }

    Ok(result)
}

/// Tauri command: Restart application
/// Uses ShellExecuteW to properly handle UAC elevation when admin launch is enabled
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    // Use our custom restart that works with UAC elevation
    if admin_launch::restart_app() {
        // Exit current process after new instance is started
        app.exit(0);
    } else {
        // Fallback to Tauri's restart
        tauri::process::restart(&app.env());
    }
}

/// 切换主窗口显示/隐藏
fn toggle_window_visibility(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if keyboard_hook::get_window_state() == keyboard_hook::WindowState::Visible {
            let _ = window.hide();
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
            input_monitor::disable_mouse_monitoring();
            commands::hide_image_preview_window(app);
            let _ = window.emit("window-hidden", ());
        } else {
            // 跟随光标定位
            let follow_cursor = app
                .try_state::<std::sync::Arc<commands::AppState>>()
                .map(|state| {
                    let settings_repo = database::SettingsRepository::new(&state.db);
                    settings_repo
                        .get("follow_cursor")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true)
                })
                .unwrap_or(true);
            if follow_cursor {
                if let Err(e) = positioning::position_at_cursor(&window) {
                    tracing::warn!("定位窗口失败: {}", e);
                }
            }

            // 显示并置顶，取得焦点以支持键盘导航
            let _ = window.show();
            positioning::force_topmost(&window);
            let _ = window.set_focus();
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
            input_monitor::enable_mouse_monitoring();
            let _ = window.emit("window-shown", ());
        }
    }
}

/// Tauri command: Enable Win+V replacement
/// This uses registry to disable system Win+V and Tauri's global_shortcut for our Win+V
#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // Remember the current shortcut so we can restore it on failure
    let saved_shortcut_str = get_current_shortcut();
    let saved_shortcut = parse_shortcut(&saved_shortcut_str);

    // Unregister current custom shortcut
    if let Some(shortcut) = saved_shortcut {
        let _ = app.global_shortcut().unregister(shortcut);
    }

    // Disable system Win+V via registry (restart explorer to apply)
    if let Err(e) = win_v_registry::disable_win_v_hotkey(true) {
        // Re-register custom shortcut before returning error
        if let Some(sc) = saved_shortcut {
            let _ = app.global_shortcut().on_shortcut(sc, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
        }
        return Err(e);
    }

    // Now register Win+V using Tauri's global_shortcut (system Win+V is disabled)
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    if let Err(e) = app.global_shortcut()
        .on_shortcut(winv_shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
    {
        // Restore: re-enable system Win+V and re-register custom shortcut
        let _ = win_v_registry::enable_win_v_hotkey(true);
        if let Some(sc) = saved_shortcut {
            let _ = app.global_shortcut().on_shortcut(sc, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
        }
        return Err(format!("Failed to register Win+V shortcut: {}", e));
    }

    // Save setting
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "true");
    Ok(())
}

/// Tauri command: Disable Win+V replacement
/// This will re-enable system Win+V and our custom shortcut
#[tauri::command]
async fn disable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // Unregister Win+V shortcut
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    let _ = app.global_shortcut().unregister(winv_shortcut);

    // Re-enable system Win+V via registry (restart explorer to apply)
    win_v_registry::enable_win_v_hotkey(true)?;

    // Re-register custom shortcut with toggle handler
    if let Some(shortcut) = parse_shortcut(&get_current_shortcut()) {
        let _ = app
            .global_shortcut()
            .on_shortcut(shortcut, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
    }

    // Save setting
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "false");
    Ok(())
}

/// Tauri command: Check if Win+V replacement is enabled
#[tauri::command]
async fn is_winv_replacement_enabled(_app: tauri::AppHandle) -> bool {
    // Check registry status
    win_v_registry::is_win_v_hotkey_disabled()
}

/// Tauri command: Update main shortcut
#[tauri::command]
async fn update_shortcut(app: tauri::AppHandle, new_shortcut: String) -> Result<String, String> {
    // Parse new shortcut
    let new_sc = parse_shortcut(&new_shortcut)
        .ok_or_else(|| format!("Invalid shortcut: {}", new_shortcut))?;

    if !shortcut_has_modifier(&new_shortcut) {
        return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
    }

    // Unregister current shortcut
    if let Some(current_sc) = parse_shortcut(&get_current_shortcut()) {
        let _ = app.global_shortcut().unregister(current_sc);
    }

    // Register new shortcut with toggle handler
    app.global_shortcut()
        .on_shortcut(new_sc, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // Update global state
    *CURRENT_SHORTCUT.write() = Some(new_shortcut.clone());

    Ok(new_shortcut)
}

/// Tauri command: Get current shortcut
#[tauri::command]
fn get_current_shortcut() -> String {
    CURRENT_SHORTCUT
        .read()
        .clone()
        .unwrap_or_else(|| "Alt+C".to_string())
}

fn reload_quick_paste_shortcuts_from_settings(app: &tauri::AppHandle) -> HashMap<u8, String> {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = SettingsRepository::new(&state.db);
    let shortcuts = load_quick_paste_shortcuts(&settings_repo);
    apply_quick_paste_shortcuts(app, &shortcuts)
}

/// Tauri command: Get quick paste shortcuts for slot 1-9
#[tauri::command]
fn get_quick_paste_shortcuts() -> Vec<String> {
    let current = CURRENT_QUICK_PASTE_SHORTCUTS.read();
    if current.len() == 9 {
        return current.clone();
    }
    default_quick_paste_shortcuts()
}

/// Tauri command: Update quick paste shortcut for one slot (1-9)
#[tauri::command]
fn set_quick_paste_shortcut(
    app: tauri::AppHandle,
    slot: u8,
    shortcut: String,
) -> Result<(), String> {
    if !(1..=9).contains(&slot) {
        return Err("slot must be between 1 and 9".to_string());
    }

    let normalized = normalize_shortcut_value(&shortcut);
    if !normalized.is_empty() {
        let parsed = parse_shortcut(&normalized)
            .ok_or_else(|| format!("Invalid shortcut: {}", normalized))?;
        if !shortcut_has_modifier(&normalized) {
            return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
        }
        // Prevent conflict with the main toggle shortcut
        let main_sc = get_current_shortcut();
        if let Some(main_parsed) = parse_shortcut(&main_sc) {
            if parsed == main_parsed {
                return Err(format!("与呼出快捷键 {} 冲突", main_sc));
            }
        }
    }

    let mut next_shortcuts = {
        let current = CURRENT_QUICK_PASTE_SHORTCUTS.read();
        if current.len() == 9 {
            current.clone()
        } else {
            default_quick_paste_shortcuts()
        }
    };
    let idx = (slot - 1) as usize;
    let previous = next_shortcuts[idx].clone();
    next_shortcuts[idx] = normalized.clone();

    let failures = apply_quick_paste_shortcuts(&app, &next_shortcuts);
    if let Some(err) = failures.get(&slot) {
        // Roll back only when the updated slot itself failed to register.
        next_shortcuts[idx] = previous;
        let _ = apply_quick_paste_shortcuts(&app, &next_shortcuts);
        return Err(err.clone());
    }

    let state = app.state::<Arc<AppState>>();
    let settings_repo = SettingsRepository::new(&state.db);
    settings_repo
        .set(&quick_paste_setting_key(slot), &normalized)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Tauri command: Set window pinned state
#[tauri::command]
fn set_window_pinned(pinned: bool) {
    input_monitor::set_window_pinned(pinned);
}

/// Tauri command: Get window pinned state
#[tauri::command]
fn is_window_pinned() -> bool {
    input_monitor::is_window_pinned()
}

/// Tauri command: Check if admin launch is enabled
#[tauri::command]
fn is_admin_launch_enabled() -> bool {
    admin_launch::is_admin_launch_enabled()
}

/// Tauri command: Enable admin launch
#[tauri::command]
fn enable_admin_launch() -> Result<(), String> {
    admin_launch::enable_admin_launch()
}

/// Tauri command: Disable admin launch
#[tauri::command]
fn disable_admin_launch() -> Result<(), String> {
    admin_launch::disable_admin_launch()
}

/// Tauri command: Check if currently running as admin
#[tauri::command]
fn is_running_as_admin() -> bool {
    admin_launch::is_running_as_admin()
}

// ============ Update Commands ============

/// Tauri command: Check GitHub for updates
#[tauri::command]
async fn check_for_update() -> Result<updater::UpdateInfo, String> {
    tokio::task::spawn_blocking(updater::check_update)
        .await
        .map_err(|e| e.to_string())?
}

/// Tauri command: Download update installer with progress events
#[tauri::command]
async fn download_update(
    app: tauri::AppHandle,
    download_url: String,
    file_name: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || updater::download(&app, &download_url, &file_name))
        .await
        .map_err(|e| e.to_string())?
}

/// Tauri command: Launch installer and exit application
#[tauri::command]
async fn install_update(app: tauri::AppHandle, installer_path: String) -> Result<(), String> {
    updater::install(&installer_path)?;
    // Brief delay to let the installer process start before exiting
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.exit(0);
    Ok(())
}

// ============ Image Preview Window ============

/// Tauri command: Show image preview in a fixed-size transparent window
/// The window fills the available space to the left (or right) of the main window.
/// Image sizing is handled by CSS inside the webview — no window resize during zoom.
#[tauri::command]
async fn show_image_preview(
    app: tauri::AppHandle,
    image_path: String,
    img_width: f64,
    img_height: f64,
    win_x: f64,
    win_y: f64,
    win_width: f64,
    win_height: f64,
) -> Result<(), String> {
    let mut newly_created = false;
    let window = if let Some(w) = app.get_webview_window("image-preview") {
        w
    } else {
        newly_created = true;
        tauri::WebviewWindowBuilder::new(
            &app,
            "image-preview",
            tauri::WebviewUrl::App("/image-preview.html".into()),
        )
        .title("")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| format!("创建预览窗口失败: {}", e))?
    };

    // 始终使用物理像素设置位置/尺寸，避免混合 DPI 换算误差
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: win_width as u32,
        height: win_height as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: win_x as i32,
        y: win_y as i32,
    }));

    if newly_created {
        // 首次创建：等待 HTML 加载后再发送事件
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    // 确保置顶生效（主窗口焦点状态可能影响窗口层级）
    let _ = window.set_always_on_top(true);

    // 发送图片路径和初始 CSS 尺寸到预览窗口
    let _ = window.emit(
        "image-preview-update",
        serde_json::json!({
            "imagePath": image_path,
            "width": img_width,
            "height": img_height,
        }),
    );

    let _ = window.show();
    Ok(())
}

/// 隐藏图片预览窗口并清除内容
#[tauri::command]
async fn hide_image_preview(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("image-preview") {
        let _ = window.hide();
        // 清除图片，避免下次显示时闪烁旧内容
        let _ = window.emit("image-preview-clear", ());
    }
}

/// 打开文本编辑器窗口
#[tauri::command]
async fn open_text_editor_window(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("text-editor-{}", id);

    // 若该条目的编辑器已存在则聚焦
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(format!("/editor?id={}", id).into()),
    )
    .title("编辑")
    .inner_size(600.0, 460.0)
    .min_inner_size(400.0, 300.0)
    .decorations(false)
    .visible(false)
    .resizable(true)
    .center()
    .build()
    .map_err(|e| format!("创建编辑器窗口失败: {}", e))?;

    // 窗口将在前端内容加载完成后显示
    let _ = window;
    Ok(())
}

/// 打开设置窗口
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    tray::open_settings_window(&app)
}

// ============ 日志设置命令 ============

/// 检查文件日志是否启用
#[tauri::command]
fn is_log_to_file_enabled() -> bool {
    AppConfig::load().is_log_to_file()
}

/// 启用或禁用文件日志（需重启生效）
#[tauri::command]
fn set_log_to_file(enabled: bool) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.log_to_file = Some(enabled);
    config.save()
}

/// 获取日志文件路径
#[tauri::command]
fn get_log_file_path() -> String {
    AppConfig::load().get_log_path().to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load();
    init_logging(&config);

    // ── 管理员自提权（在 Tauri Builder 之前） ─────────────────────────────
    // 若启用了 run_as_admin 但当前未提权，则启动新的提权实例并退出当前进程。
    // 提权优先使用预创建的计划任务（免 UAC），失败则回退到 UAC 弹窗。
    #[cfg(target_os = "windows")]
    {
        if config.run_as_admin.unwrap_or(false) {
            if admin_launch::is_running_as_admin() {
                // 已提权 → 确保计划任务存在，后续重启可免 UAC
                let _ = task_scheduler::create_elevation_task();
            } else if admin_launch::self_elevate() {
                // 新提权实例已启动 → 退出当前进程
                std::process::exit(0);
            }
            // 提权失败则继续以非管理员运行
        }

        // 迁移清理：删除旧版 ONLOGON 计划任务和 AppCompatFlags 注册表项
        // （幂等操作，不存在时安全跳过）
        task_scheduler::delete_legacy_autostart_task();
        admin_launch::cleanup_compat_flags();
    }

    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri_plugin_notification::NotificationExt;
            let _ = app
                .notification()
                .builder()
                .title("ElegantClipboard")
                .body("程序已在运行中")
                .show();
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // 加载配置并初始化数据库
            let config = AppConfig::load();
            let db_path = config.get_db_path();
            let images_path = config.get_images_path();
            let db = Database::new(db_path).map_err(|e| e.to_string())?;

            // 初始化剪贴板监控
            let monitor = ClipboardMonitor::new();
            monitor.init(&db, images_path);

            let state = Arc::new(AppState { db, monitor });

            // 从数据库加载已保存的快捷键
            let settings_repo = database::SettingsRepository::new(&state.db);
            let saved_shortcut = settings_repo
                .get("global_shortcut")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Alt+C".to_string());

            // 启动剪贴板监控
            state.monitor.start(app.handle().clone());
            app.manage(state);

            // 初始化系统托盘
            let _ = tray::setup_tray(app.handle());

            // 注册全局快捷键（根据 Win+V 替换设置选择快捷键）
            *CURRENT_SHORTCUT.write() = Some(saved_shortcut.clone());
            let shortcut = if win_v_registry::is_win_v_hotkey_disabled() {
                Shortcut::new(Some(Modifiers::SUPER), Code::KeyV)
            } else {
                parse_shortcut(&saved_shortcut)
                    .unwrap_or_else(|| Shortcut::new(Some(Modifiers::ALT), Code::KeyC))
            };

            let _ = app
                .global_shortcut()
                .on_shortcut(shortcut, |app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_window_visibility(app);
                    }
                });

            // 注册快速粘贴快捷键（槽位 1-9）
            let quick_paste_failures = reload_quick_paste_shortcuts_from_settings(app.handle());
            for (slot, err) in quick_paste_failures {
                tracing::warn!("快速粘贴快捷键注册失败（槽位 {}）: {}", slot, err);
            }

            // 设置主窗口为不可聚焦，避免抢占其他应用焦点
            // 同时使快捷键在开始菜单等系统 UI 打开时仍可用
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focusable(false);

                // 初始化全局鼠标监控（用于点击外部隐藏窗口）
                // 不可聚焦窗口无法触发 onFocusChanged，因此需要此机制
                input_monitor::init(window);
                input_monitor::start_monitoring();
            }

            // 启动系统强调色监听（实时主题更新）
            #[cfg(target_os = "windows")]
            commands::settings::start_accent_color_watcher(app.handle().clone());

            // 发送启动通知
            {
                use tauri_plugin_notification::NotificationExt;
                let shortcut_display = if win_v_registry::is_win_v_hotkey_disabled() {
                    "Win+V".to_string()
                } else {
                    saved_shortcut.clone()
                };
                let _ = app
                    .notification()
                    .builder()
                    .title("ElegantClipboard 已启动")
                    .body(format!(
                        "程序已在后台运行，按 {} 打开剪贴板",
                        shortcut_display
                    ))
                    .show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 窗口命令
            get_app_version,
            get_default_data_path,
            get_original_default_path,
            set_data_path,
            migrate_data_to_path,
            restart_app,
            show_window,
            hide_window,
            set_window_visibility,
            minimize_window,
            toggle_maximize,
            close_window,
            open_settings_window,
            show_image_preview,
            hide_image_preview,
            open_text_editor_window,
            set_window_pinned,
            is_window_pinned,
            // 管理员启动命令
            is_admin_launch_enabled,
            enable_admin_launch,
            disable_admin_launch,
            is_running_as_admin,
            // 日志命令
            is_log_to_file_enabled,
            set_log_to_file,
            get_log_file_path,
            // 快捷键命令
            enable_winv_replacement,
            disable_winv_replacement,
            is_winv_replacement_enabled,
            update_shortcut,
            get_current_shortcut,
            get_quick_paste_shortcuts,
            set_quick_paste_shortcut,
            // 更新命令
            check_for_update,
            download_update,
            install_update,
            // 剪贴板命令
            commands::clipboard::get_clipboard_items,
            commands::clipboard::get_clipboard_item,
            commands::clipboard::get_clipboard_count,
            commands::clipboard::toggle_pin,
            commands::clipboard::toggle_favorite,
            commands::clipboard::move_clipboard_item,
            commands::clipboard::delete_clipboard_item,
            commands::clipboard::clear_history,
            commands::clipboard::copy_to_clipboard,
            commands::clipboard::paste_content,
            commands::clipboard::paste_content_as_plain,
            commands::clipboard::paste_text_direct,
            commands::clipboard::update_text_content,
            // 设置、监控、数据库、文件夹、自启动命令
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            commands::settings::pause_monitor,
            commands::settings::resume_monitor,
            commands::settings::get_monitor_status,
            commands::settings::optimize_database,
            commands::settings::vacuum_database,
            commands::settings::select_folder_for_settings,
            commands::settings::open_data_folder,
            commands::settings::is_autostart_enabled,
            commands::settings::enable_autostart,
            commands::settings::disable_autostart,
            commands::settings::get_system_accent_color,
            // 文件操作命令
            commands::file_ops::check_files_exist,
            commands::file_ops::show_in_explorer,
            commands::file_ops::paste_as_path,
            commands::file_ops::get_file_details,
            commands::file_ops::save_file_as,
            commands::file_ops::get_data_size,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = run_result {
        eprintln!("error while running tauri application: {err}");
    }
}
