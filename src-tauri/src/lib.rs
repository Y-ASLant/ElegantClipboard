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

struct LocalTimer;
impl tracing_subscriber::fmt::time::FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}
static CURRENT_SHORTCUT: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);
static CURRENT_QUICK_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> =
    parking_lot::RwLock::new(Vec::new());
static QUICK_PASTE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
static ACTIVE_QUICK_PASTE_SLOTS: std::sync::LazyLock<parking_lot::Mutex<HashSet<u8>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashSet::new()));
/// simulate_paste 释放修饰键时可能导致 OS 重新触发快捷键，用此标志拦截假触发
static PASTE_IN_PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
                    let any_focused = app
                        .webview_windows()
                        .values()
                        .any(|w| w.is_focused().unwrap_or(false));
                    if any_focused {
                        return;
                    }

                    if PASTE_IN_PROGRESS.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }

                    let is_first = {
                        let mut active = ACTIVE_QUICK_PASTE_SLOTS.lock();
                        active.insert(slot) // true = newly inserted
                    };

                    let state = app.state::<Arc<AppState>>().inner().clone();
                    let app_handle = app.clone();
                    std::thread::spawn(move || {
                        let _guard = QUICK_PASTE_LOCK.lock();

                        PASTE_IN_PROGRESS.store(true, std::sync::atomic::Ordering::Release);

                        if is_first {
                            if let Err(err) = commands::clipboard::quick_paste_by_slot(
                                &state, &app_handle, slot,
                            ) {
                                tracing::warn!("Quick paste slot {} failed: {}", slot, err);
                                ACTIVE_QUICK_PASTE_SLOTS.lock().remove(&slot);
                            }
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            if let Err(err) = commands::clipboard::simulate_paste() {
                                tracing::warn!("Quick paste repeat slot {} failed: {}", slot, err);
                            }
                        }

                        PASTE_IN_PROGRESS.store(false, std::sync::atomic::Ordering::Release);
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

static FILE_LOG_GUARD: parking_lot::Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
    parking_lot::Mutex::new(None);

fn rotate_log_if_needed(log_path: &std::path::Path, max_size: u64) {
    if let Ok(meta) = std::fs::metadata(log_path) {
        if meta.len() > max_size {
            let backup = log_path.with_extension("log.old");
            let _ = std::fs::rename(log_path, backup);
        }
    }
}

fn init_logging(config: &AppConfig) {
    let stdout_layer = fmt::layer()
        .with_timer(LocalTimer)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    let file_layer = if config.is_log_to_file() {
        let log_path = config.get_log_path();
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
                        .with_timer(LocalTimer)
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

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn show_window(window: tauri::WebviewWindow) {
    let _ = window.show();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
    let _ = window.emit("window-shown", ());
}

#[tauri::command]
async fn hide_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
    input_monitor::disable_mouse_monitoring();
    commands::hide_image_preview_window(window.app_handle());
    let _ = window.emit("window-hidden", ());
}

#[tauri::command]
fn set_window_visibility(visible: bool) {
    keyboard_hook::set_window_state(if visible {
        keyboard_hook::WindowState::Visible
    } else {
        keyboard_hook::WindowState::Hidden
    });
    if visible {
        input_monitor::enable_mouse_monitoring();
    } else {
        input_monitor::disable_mouse_monitoring();
    }
}

#[tauri::command]
async fn minimize_window(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

#[tauri::command]
async fn toggle_maximize(window: tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
async fn close_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
    input_monitor::disable_mouse_monitoring();
    commands::hide_image_preview_window(window.app_handle());
    let _ = window.emit("window-hidden", ());
}

#[tauri::command]
fn get_default_data_path() -> String {
    let config = AppConfig::load();
    config.get_data_dir().to_string_lossy().to_string()
}

#[tauri::command]
fn get_original_default_path() -> String {
    database::get_default_db_path()
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
fn check_path_has_data(path: String) -> bool {
    let p = std::path::PathBuf::from(&path);
    p.join("clipboard.db").exists()
}

#[tauri::command]
fn cleanup_data_at_path(path: String) -> Result<(), String> {
    use std::fs;
    let p = std::path::PathBuf::from(&path);

    for ext in &["", "-wal", "-shm"] {
        let db_file = p.join(format!("clipboard.db{}", ext));
        if db_file.exists() {
            fs::remove_file(&db_file).map_err(|e| format!("删除 {:?} 失败: {}", db_file, e))?;
        }
    }

    let images_dir = p.join("images");
    if images_dir.exists() {
        fs::remove_dir_all(&images_dir)
            .map_err(|e| format!("删除图片目录失败: {}", e))?;
    }

    let icons_dir = p.join("icons");
    if icons_dir.exists() {
        fs::remove_dir_all(&icons_dir)
            .map_err(|e| format!("删除图标目录失败: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
fn set_data_path(path: String) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.data_path = if path.is_empty() { None } else { Some(path) };
    config.save()
}

#[tauri::command]
fn migrate_data_to_path(new_path: String) -> Result<config::MigrationResult, String> {
    let config = AppConfig::load();
    let old_path = config.get_data_dir();
    let new_path = std::path::PathBuf::from(&new_path);

    if old_path == new_path {
        return Err("Source and destination paths are the same".to_string());
    }

    let result = config::migrate_data(&old_path, &new_path)?;

    if result.success() {
        let mut new_config = AppConfig::load();
        new_config.data_path = Some(new_path.to_string_lossy().to_string());
        new_config.save()?;
    }

    Ok(result)
}

#[tauri::command]
async fn export_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<commands::AppState>>,
) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    let export_db = data_dir.join("clipboard.db.export");
    {
        let src_conn = state.db.write_connection();
        let src_conn = src_conn.lock();
        let _ = fs::remove_file(&export_db);
        let mut dst_conn = rusqlite::Connection::open(&export_db)
            .map_err(|e| format!("创建备份文件失败: {}", e))?;
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)
            .map_err(|e| format!("初始化备份失败: {}", e))?;
        backup
            .run_to_completion(100, std::time::Duration::from_millis(0), None)
            .map_err(|e| format!("执行备份失败: {}", e))?;
    }

    let timestamp = chrono_timestamp();
    let default_name = format!("ElegantClipboard_backup_{}.zip", timestamp);
    let dest = app
        .dialog()
        .file()
        .set_title("导出数据")
        .set_file_name(&default_name)
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_save_file();

    let dest_path = match dest {
        Some(p) => p.to_string(),
        None => {
            let _ = fs::remove_file(&export_db);
            return Err("用户取消了导出".to_string());
        }
    };

    let file = File::create(&dest_path).map_err(|e| format!("创建文件失败: {}", e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("clipboard.db", options).map_err(|e| e.to_string())?;
    zip.write_all(&fs::read(&export_db).map_err(|e| format!("读取数据库副本失败: {}", e))?)
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&export_db);

    add_dir_to_zip(&mut zip, &data_dir.join("images"), "images", options)?;

    add_dir_to_zip(&mut zip, &data_dir.join("icons"), "icons", options)?;

    zip.finish().map_err(|e| e.to_string())?;

    let size = fs::metadata(&dest_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(format!("导出成功 ({})", format_size(size)))
}

#[tauri::command]
async fn import_data(app: tauri::AppHandle) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Read;
    use tauri_plugin_dialog::DialogExt;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    let src = app
        .dialog()
        .file()
        .set_title("导入数据")
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_pick_file();

    let src_path = match src {
        Some(p) => p.to_string(),
        None => return Err("用户取消了导入".to_string()),
    };

    let file = File::open(&src_path).map_err(|e| format!("打开文件失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("无效的 ZIP 文件: {}", e))?;

    let has_db = (0..archive.len()).any(|i| {
        archive
            .by_index(i)
            .map(|f| f.name() == "clipboard.db")
            .unwrap_or(false)
    });
    if !has_db {
        return Err("ZIP 文件中未找到 clipboard.db，不是有效的备份文件".to_string());
    }

    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let mut files_extracted = 0u32;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        // 安全检查：防止路径穿越
        if name.contains("..") {
            continue;
        }

        // 跳过 WAL 临时文件；clipboard.db 写为 staging 文件，启动时替换
        if name.ends_with("-wal") || name.ends_with("-shm") {
            continue;
        }
        let out_path = if name == "clipboard.db" {
            data_dir.join("clipboard.db.import")
        } else {
            data_dir.join(&name)
        };

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            fs::write(&out_path, &buf)
                .map_err(|e| format!("写入 {} 失败: {}", name, e))?;
            files_extracted += 1;
        }
    }

    Ok(format!("导入成功，共恢复 {} 个文件，应用即将重启", files_extracted))
}

/// 检测并应用待导入的 staging 数据库文件（clipboard.db.import → clipboard.db）。
fn apply_pending_import(db_path: &std::path::Path) {
    use std::fs;

    let staging = db_path.with_extension("db.import");
    if !staging.exists() {
        return;
    }

    tracing::info!("Detected pending import: {:?}", staging);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let db_dir = match db_path.parent() {
        Some(d) => d,
        None => return,
    };

    for attempt in 1..=10 {
        let deleted = ["", "-wal", "-shm"].iter().all(|ext| {
            let f = db_dir.join(format!("clipboard.db{ext}"));
            !f.exists() || fs::remove_file(&f).is_ok()
        });

        if deleted && fs::rename(&staging, db_path).is_ok() {
            tracing::info!("Import staging applied (attempt {attempt})");
            return;
        }

        if attempt < 10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    tracing::error!("Rename failed after 10 attempts, trying copy fallback");
    if fs::copy(&staging, db_path).is_ok() {
        let _ = fs::remove_file(&staging);
        tracing::info!("Import applied via copy fallback");
    } else {
        tracing::error!("Import staging completely failed");
    }
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    use std::io::Write;

    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let buf = std::fs::read(&path)
                .map_err(|e| format!("读取 {:?} 失败: {}", path, e))?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn chrono_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = now + 8 * 3600;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let (y, mo, d) = days_to_ymd(days);
    format!("{:04}{:02}{:02}_{:02}{:02}{:02}", y, mo, d, h, m, s)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut y = 1970;
    loop {
        let yd = if is_leap(y) { 366 } else { 365 };
        if days < yd {
            break;
        }
        days -= yd;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut mo = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            mo = i as u64 + 1;
            break;
        }
        days -= md;
    }
    (y, mo, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    if admin_launch::restart_app() {
        app.exit(0);
    } else {
        tauri::process::restart(&app.env());
    }
}

/// 若「记住窗口大小」开关启用，将当前窗口逻辑尺寸保存到 settings 表。
/// 所有隐藏主窗口的路径都应在 hide 前调用此函数。
pub(crate) fn save_window_size_if_enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, window: &tauri::WebviewWindow<R>) {
    if let Some(state) = app.try_state::<std::sync::Arc<commands::AppState>>() {
        let settings_repo = database::SettingsRepository::new(&state.db);
        let persist = settings_repo.get("persist_window_size").ok().flatten()
            .map(|v| v != "false").unwrap_or(true);
        if persist {
            if let Ok(size) = window.inner_size() {
                if let Ok(scale) = window.scale_factor() {
                    let w = (size.width as f64 / scale).round() as u32;
                    let h = (size.height as f64 / scale).round() as u32;
                    let _ = settings_repo.set("window_width", &w.to_string());
                    let _ = settings_repo.set("window_height", &h.to_string());
                }
            }
        }
    }
}

/// 切换主窗口显示/隐藏
fn toggle_window_visibility(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if keyboard_hook::get_window_state() == keyboard_hook::WindowState::Visible {
            save_window_size_if_enabled(app, &window);

            let _ = window.set_focusable(false);
            let _ = window.hide();
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
            input_monitor::disable_mouse_monitoring();
            commands::hide_image_preview_window(app);
            let _ = window.emit("window-hidden", ());
        } else {
            let follow_cursor = app
                .try_state::<std::sync::Arc<commands::AppState>>()
                .map(|state| {
                    let repo = database::SettingsRepository::new(&state.db);
                    let persist = repo.get("persist_window_size").ok().flatten()
                        .map(|v| v != "false").unwrap_or(true);
                    if persist {
                        let w = repo.get("window_width").ok().flatten()
                            .and_then(|v| v.parse::<f64>().ok());
                        let h = repo.get("window_height").ok().flatten()
                            .and_then(|v| v.parse::<f64>().ok());
                        if let (Some(w), Some(h)) = (w, h) {
                            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                                width: w,
                                height: h,
                            }));
                        }
                    }
                    repo.get("follow_cursor").ok().flatten()
                        .map(|v| v != "false").unwrap_or(true)
                })
                .unwrap_or(true);

            if follow_cursor {
                if let Err(e) = positioning::position_at_cursor(&window) {
                    tracing::warn!("定位窗口失败: {}", e);
                }
            }

            input_monitor::save_current_focus();
            let _ = window.show();
            positioning::force_topmost(&window);
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
            input_monitor::enable_mouse_monitoring();
            let _ = window.emit("window-shown", ());
        }
    }
}

#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    let saved_shortcut_str = get_current_shortcut();
    let saved_shortcut = parse_shortcut(&saved_shortcut_str);

    if let Some(shortcut) = saved_shortcut {
        let _ = app.global_shortcut().unregister(shortcut);
    }

    if let Err(e) = win_v_registry::disable_win_v_hotkey(true) {
        if let Some(sc) = saved_shortcut {
            let _ = app.global_shortcut().on_shortcut(sc, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
        }
        return Err(e);
    }
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    if let Err(e) = app.global_shortcut()
        .on_shortcut(winv_shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
    {
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

    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "true");
    Ok(())
}

#[tauri::command]
async fn disable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    let _ = app.global_shortcut().unregister(winv_shortcut);

    win_v_registry::enable_win_v_hotkey(true)?;

    if let Some(shortcut) = parse_shortcut(&get_current_shortcut()) {
        let _ = app
            .global_shortcut()
            .on_shortcut(shortcut, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
    }

    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "false");
    Ok(())
}

#[tauri::command]
async fn is_winv_replacement_enabled(_app: tauri::AppHandle) -> bool {
    win_v_registry::is_win_v_hotkey_disabled()
}

#[tauri::command]
async fn update_shortcut(app: tauri::AppHandle, new_shortcut: String) -> Result<String, String> {
    let new_sc = parse_shortcut(&new_shortcut)
        .ok_or_else(|| format!("Invalid shortcut: {}", new_shortcut))?;

    if !shortcut_has_modifier(&new_shortcut) {
        return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
    }

    if let Some(current_sc) = parse_shortcut(&get_current_shortcut()) {
        let _ = app.global_shortcut().unregister(current_sc);
    }

    app.global_shortcut()
        .on_shortcut(new_sc, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    *CURRENT_SHORTCUT.write() = Some(new_shortcut.clone());

    Ok(new_shortcut)
}

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

#[tauri::command]
fn get_quick_paste_shortcuts() -> Vec<String> {
    let current = CURRENT_QUICK_PASTE_SHORTCUTS.read();
    if current.len() == 9 {
        return current.clone();
    }
    default_quick_paste_shortcuts()
}

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
        let upper = normalized.to_uppercase();
        if upper.split('+').any(|p| matches!(p.trim(), "WIN" | "SUPER" | "META" | "CMD")) {
            return Err("快速粘贴不支持 Win 修饰键（Win+数字 是系统任务栏快捷键）".to_string());
        }
        let parsed = parse_shortcut(&normalized)
            .ok_or_else(|| format!("Invalid shortcut: {}", normalized))?;
        if !shortcut_has_modifier(&normalized) {
            return Err("快捷键至少包含一个修饰键 (Ctrl/Alt)".to_string());
        }
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

#[tauri::command]
async fn set_window_pinned(window: tauri::WebviewWindow, pinned: bool) {
    input_monitor::set_window_pinned(pinned);
    if pinned {
        let _ = window.set_focusable(false);
        #[cfg(windows)]
        {
            let prev = input_monitor::get_prev_foreground_hwnd();
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
fn is_window_pinned() -> bool {
    input_monitor::is_window_pinned()
}

#[tauri::command]
fn set_window_effect(window: tauri::WebviewWindow, effect: String, dark: Option<bool>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, WS_EX_LAYERED,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        };

        let raw_hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let hwnd = HWND(raw_hwnd.0 as *mut _);

        let is_effect = effect != "none";

        unsafe {
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let has_layered = (ex_style as u32) & WS_EX_LAYERED.0 != 0;

            if is_effect && has_layered {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ((ex_style as u32) & !WS_EX_LAYERED.0) as i32);
                let _ = SetWindowPos(
                    hwnd, None, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            } else if !is_effect && !has_layered {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ((ex_style as u32) | WS_EX_LAYERED.0) as i32);
                let _ = SetWindowPos(
                    hwnd, None, 0, 0, 0, 0,
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
            // Restore WS_EX_LAYERED — we may have removed it before the failed attempt
            unsafe {
                let cur_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                if (cur_style as u32) & WS_EX_LAYERED.0 == 0 {
                    SetWindowLongW(
                        hwnd, GWL_EXSTYLE,
                        ((cur_style as u32) | WS_EX_LAYERED.0) as i32,
                    );
                    let _ = SetWindowPos(
                        hwnd, None, 0, 0, 0, 0,
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
async fn focus_clipboard_window(window: tauri::WebviewWindow) {
    input_monitor::focus_clipboard_window(&window);
}

#[tauri::command]
async fn restore_last_focus(window: tauri::WebviewWindow) {
    input_monitor::restore_last_focus(&window);
}

#[tauri::command]
fn save_current_focus() {
    input_monitor::save_current_focus();
}

#[tauri::command]
async fn set_keyboard_nav_enabled(window: tauri::WebviewWindow, enabled: bool) {
    input_monitor::set_keyboard_nav_enabled(enabled);
    // 不再因键盘导航切换而抢焦点，导航键通过低级钩子转发
    if !enabled && window.is_visible().unwrap_or(false) && !input_monitor::is_window_pinned() {
        // 关闭时若窗口仍聚焦则恢复
        if window.is_focused().unwrap_or(false) {
            input_monitor::restore_last_focus(&window);
        }
    }
}

#[tauri::command]
fn is_admin_launch_enabled() -> bool {
    admin_launch::is_admin_launch_enabled()
}

#[tauri::command]
fn enable_admin_launch() -> Result<(), String> {
    admin_launch::enable_admin_launch()
}

#[tauri::command]
fn disable_admin_launch() -> Result<(), String> {
    admin_launch::disable_admin_launch()
}

#[tauri::command]
fn is_running_as_admin() -> bool {
    admin_launch::is_running_as_admin()
}

#[tauri::command]
async fn check_for_update() -> Result<updater::UpdateInfo, String> {
    tokio::task::spawn_blocking(updater::check_update)
        .await
        .map_err(|e| e.to_string())?
}

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

#[tauri::command]
fn cancel_update_download() {
    updater::cancel_download();
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle, installer_path: String) -> Result<(), String> {
    updater::install(&installer_path)?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.exit(0);
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
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

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: win_width as u32,
        height: win_height as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: win_x as i32,
        y: win_y as i32,
    }));

    if newly_created {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let _ = window.set_always_on_top(true);

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

#[tauri::command]
async fn hide_image_preview(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("image-preview") {
        let _ = window.hide();
        let _ = window.emit("image-preview-clear", ());
    }
}

#[tauri::command]
async fn open_text_editor_window(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("text-editor-{}", id);

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

    let _ = window;
    Ok(())
}

#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    tray::open_settings_window(&app)
}

#[tauri::command]
fn is_log_to_file_enabled() -> bool {
    AppConfig::load().is_log_to_file()
}

#[tauri::command]
fn set_log_to_file(enabled: bool) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.log_to_file = Some(enabled);
    config.save()
}

#[tauri::command]
fn get_log_file_path() -> String {
    AppConfig::load().get_log_path().to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load();
    init_logging(&config);

    match wry::webview_version() {
        Ok(ver) => tracing::info!("WebView2 runtime version: {}", ver),
        Err(e) => tracing::warn!("WebView2 version query failed: {}", e),
    }

    #[cfg(target_os = "windows")]
    {
        if config.run_as_admin.unwrap_or(false) {
            if admin_launch::is_running_as_admin() {
                let _ = task_scheduler::create_elevation_task();
            } else if admin_launch::self_elevate() {
                std::process::exit(0);
            }
        }

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
            let config = AppConfig::load();
            let db_path = config.get_db_path();
            let images_path = config.get_images_path();

            apply_pending_import(&db_path);

            let db = Database::new(db_path).map_err(|e| e.to_string())?;

            let monitor = ClipboardMonitor::new();
            monitor.init(&db, images_path);

            let state = Arc::new(AppState { db, monitor });

            let settings_repo = database::SettingsRepository::new(&state.db);
            let saved_shortcut = settings_repo
                .get("global_shortcut")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Alt+C".to_string());

            state.monitor.start(app.handle().clone());
            app.manage(state);

            let _ = tray::setup_tray(app.handle());

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

            let quick_paste_failures = reload_quick_paste_shortcuts_from_settings(app.handle());
            for (slot, err) in quick_paste_failures {
                tracing::warn!("快速粘贴快捷键注册失败（槽位 {}）: {}", slot, err);
            }

            if let Some(window) = app.get_webview_window("main") {
                let persist = settings_repo.get("persist_window_size").ok().flatten()
                    .map(|v| v != "false").unwrap_or(true);
                if persist {
                    let custom_width = settings_repo.get("window_width").ok().flatten()
                        .and_then(|v| v.parse::<f64>().ok());
                    let custom_height = settings_repo.get("window_height").ok().flatten()
                        .and_then(|v| v.parse::<f64>().ok());
                    if let (Some(w), Some(h)) = (custom_width, custom_height) {
                        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
                            width: w,
                            height: h,
                        }));
                    }
                }

                let _ = window.set_focusable(false);

                #[cfg(target_os = "windows")]
                {
                    // Ensure WS_EX_LAYERED is set at startup so the window is
                    // opaque before initTheme() runs. This prevents a transparent
                    // flash on Windows 10 where DWM effects are not supported.
                    {
                        use windows::Win32::Foundation::HWND;
                        use windows::Win32::UI::WindowsAndMessaging::{
                            GetWindowLongW, SetWindowLongW, SetWindowPos,
                            GWL_EXSTYLE, WS_EX_LAYERED,
                            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
                            SWP_NOSIZE, SWP_NOZORDER,
                        };
                        if let Ok(raw_hwnd) = window.hwnd() {
                            let hwnd = HWND(raw_hwnd.0 as *mut _);
                            unsafe {
                                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                                if (ex_style as u32) & WS_EX_LAYERED.0 == 0 {
                                    SetWindowLongW(
                                        hwnd, GWL_EXSTYLE,
                                        ((ex_style as u32) | WS_EX_LAYERED.0) as i32,
                                    );
                                    let _ = SetWindowPos(
                                        hwnd, None, 0, 0, 0, 0,
                                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER
                                            | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                                    );
                                }
                            }
                        }
                    }

                    let dpi_ctx = unsafe {
                        windows::Win32::UI::HiDpi::GetThreadDpiAwarenessContext()
                    };
                    let awareness = unsafe {
                        windows::Win32::UI::HiDpi::GetAwarenessFromDpiAwarenessContext(dpi_ctx)
                    };
                    tracing::info!("Main thread DPI awareness: {:?}", awareness);
                    if let Ok(dpi) = window.scale_factor() {
                        tracing::info!("Window scale factor: {}", dpi);
                    }
                }

                input_monitor::init(window);
                input_monitor::start_monitoring();
            }

            #[cfg(target_os = "windows")]
            commands::settings::start_accent_color_watcher(app.handle().clone());

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
            get_app_version,
            get_default_data_path,
            get_original_default_path,
            check_path_has_data,
            cleanup_data_at_path,
            set_data_path,
            migrate_data_to_path,
            export_data,
            import_data,
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
            set_window_effect,
            focus_clipboard_window,
            restore_last_focus,
            save_current_focus,
            set_keyboard_nav_enabled,
            is_admin_launch_enabled,
            enable_admin_launch,
            disable_admin_launch,
            is_running_as_admin,
            is_log_to_file_enabled,
            set_log_to_file,
            get_log_file_path,
            enable_winv_replacement,
            disable_winv_replacement,
            is_winv_replacement_enabled,
            update_shortcut,
            get_current_shortcut,
            get_quick_paste_shortcuts,
            set_quick_paste_shortcut,
            check_for_update,
            download_update,
            cancel_update_download,
            install_update,
            commands::clipboard::get_clipboard_items,
            commands::clipboard::get_clipboard_item,
            commands::clipboard::get_clipboard_count,
            commands::clipboard::toggle_pin,
            commands::clipboard::toggle_favorite,
            commands::clipboard::move_clipboard_item,
            commands::clipboard::bump_item_to_top,
            commands::clipboard::delete_clipboard_item,
            commands::clipboard::clear_history,
            commands::clipboard::clear_all_history,
            commands::clipboard::copy_to_clipboard,
            commands::clipboard::paste_content,
            commands::clipboard::paste_content_as_plain,
            commands::clipboard::paste_text_direct,
            commands::clipboard::update_text_content,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            commands::settings::pause_monitor,
            commands::settings::resume_monitor,
            commands::settings::get_monitor_status,
            commands::settings::optimize_database,
            commands::settings::vacuum_database,
            commands::settings::reset_settings,
            commands::settings::reset_all_data,
            commands::settings::select_folder_for_settings,
            commands::settings::open_data_folder,
            commands::settings::is_autostart_enabled,
            commands::settings::enable_autostart,
            commands::settings::disable_autostart,
            commands::settings::get_system_accent_color,
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
