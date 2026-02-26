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

/// 本地时间格式化器，替代默认的 UTC 时间
struct LocalTimer;
impl tracing_subscriber::fmt::time::FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}
/// 当前全局快捷键（使用 parking_lot::RwLock）
static CURRENT_SHORTCUT: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);
/// 快速粘贴快捷键缓存（槽位 1-9）
static CURRENT_QUICK_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> =
    parking_lot::RwLock::new(Vec::new());
/// 快速粘贴操作互斥锁，防止并发事件竞争系统剪贴板
static QUICK_PASTE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
/// 记录当前处于“按住”状态的快速粘贴槽位。首次按下执行完整流程，重复按下仅模拟粘贴。
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
    // Shift 单独不构成有效修饰键（如 Shift+1 输入 '!'）
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
    // 先注销之前激活的快速粘贴快捷键
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
                    // 若本应用窗口获得焦点则跳过，避免将 Ctrl+V 发送到自身 UI
                    let any_focused = app
                        .webview_windows()
                        .values()
                        .any(|w| w.is_focused().unwrap_or(false));
                    if any_focused {
                        return;
                    }

                    // 区分首次按下与按住重复
                    let is_first = {
                        let mut active = ACTIVE_QUICK_PASTE_SLOTS.lock();
                        active.insert(slot) // true = newly inserted
                    };

                    let state = app.state::<Arc<AppState>>().inner().clone();
                    let app_handle = app.clone();
                    std::thread::spawn(move || {
                        // 串行化执行，防止并发事件竞争系统剪贴板
                        let _guard = QUICK_PASTE_LOCK.lock();

                        if is_first {
                            // 完整流程：读 DB → 写剪贴板 → 粘贴
                            if let Err(err) = commands::clipboard::quick_paste_by_slot(
                                &state, &app_handle, slot,
                            ) {
                                tracing::warn!("Quick paste slot {} failed: {}", slot, err);
                                ACTIVE_QUICK_PASTE_SLOTS.lock().remove(&slot);
                            }
                        } else {
                            // 重复按下：内容已在剪贴板，仅模拟 Ctrl+V
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

/// 非阻塞写入 guard，需在整个进程生命周期内保持存活。
static FILE_LOG_GUARD: parking_lot::Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
    parking_lot::Mutex::new(None);

/// 若日志文件超过大小上限则轮转（重命名为 app.log.old）。
fn rotate_log_if_needed(log_path: &std::path::Path, max_size: u64) {
    if let Ok(meta) = std::fs::metadata(log_path) {
        if meta.len() > max_size {
            let backup = log_path.with_extension("log.old");
            let _ = std::fs::rename(log_path, backup);
        }
    }
}

/// 初始化日志系统。log_to_file 启用时写入文件（超过 10MB 自动轮转）。
fn init_logging(config: &AppConfig) {
    let stdout_layer = fmt::layer()
        .with_timer(LocalTimer)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    let file_layer = if config.is_log_to_file() {
        let log_path = config.get_log_path();
        // 确保父目录存在
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

/// Tauri 命令：获取应用版本
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Tauri 命令：显示主窗口
#[tauri::command]
async fn show_window(window: tauri::WebviewWindow) {
    let _ = window.show();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
    // 向前端发射事件以刷新缓存
    let _ = window.emit("window-shown", ());
}

/// Tauri 命令：隐藏主窗口
#[tauri::command]
async fn hide_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
    input_monitor::disable_mouse_monitoring();
    // 隐藏图片预览窗口
    commands::hide_image_preview_window(window.app_handle());
    // 向前端发射事件以重置隐藏状态
    let _ = window.emit("window-hidden", ());
}

/// Tauri 命令：设置窗口可见状态（与后端同步）
#[tauri::command]
fn set_window_visibility(visible: bool) {
    keyboard_hook::set_window_state(if visible {
        keyboard_hook::WindowState::Visible
    } else {
        keyboard_hook::WindowState::Hidden
    });
    // 同步启用/禁用鼠标监控（点击外部检测）
    if visible {
        input_monitor::enable_mouse_monitoring();
    } else {
        input_monitor::disable_mouse_monitoring();
    }
}

/// Tauri 命令：最小化窗口
#[tauri::command]
async fn minimize_window(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

/// Tauri 命令：切换最大化
#[tauri::command]
async fn toggle_maximize(window: tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

/// Tauri 命令：关闭窗口（隐藏到托盘）
#[tauri::command]
async fn close_window(window: tauri::WebviewWindow) {
    save_window_size_if_enabled(window.app_handle(), &window);
    let _ = window.set_focusable(false);
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
    input_monitor::disable_mouse_monitoring();
    // 隐藏图片预览窗口
    commands::hide_image_preview_window(window.app_handle());
    let _ = window.emit("window-hidden", ());
}

/// Tauri 命令：获取当前配置的数据路径
#[tauri::command]
fn get_default_data_path() -> String {
    let config = AppConfig::load();
    config.get_data_dir().to_string_lossy().to_string()
}

/// Tauri 命令：获取原始默认数据路径（不依赖配置）
#[tauri::command]
fn get_original_default_path() -> String {
    database::get_default_db_path()
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Tauri 命令：检测目标路径是否已有数据
#[tauri::command]
fn check_path_has_data(path: String) -> bool {
    let p = std::path::PathBuf::from(&path);
    p.join("clipboard.db").exists()
}

/// Tauri 命令：清理指定路径的数据文件（数据库、图片）
#[tauri::command]
fn cleanup_data_at_path(path: String) -> Result<(), String> {
    use std::fs;
    let p = std::path::PathBuf::from(&path);

    // 删除数据库相关文件
    for ext in &["", "-wal", "-shm"] {
        let db_file = p.join(format!("clipboard.db{}", ext));
        if db_file.exists() {
            fs::remove_file(&db_file).map_err(|e| format!("删除 {:?} 失败: {}", db_file, e))?;
        }
    }

    // 删除图片目录
    let images_dir = p.join("images");
    if images_dir.exists() {
        fs::remove_dir_all(&images_dir)
            .map_err(|e| format!("删除图片目录失败: {}", e))?;
    }

    // 删除图标缓存目录
    let icons_dir = p.join("icons");
    if icons_dir.exists() {
        fs::remove_dir_all(&icons_dir)
            .map_err(|e| format!("删除图标目录失败: {}", e))?;
    }

    Ok(())
}

/// Tauri 命令：设置数据路径并保存配置
#[tauri::command]
fn set_data_path(path: String) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.data_path = if path.is_empty() { None } else { Some(path) };
    config.save()
}

/// Tauri 命令：将数据迁移到新路径
#[tauri::command]
fn migrate_data_to_path(new_path: String) -> Result<config::MigrationResult, String> {
    let config = AppConfig::load();
    let old_path = config.get_data_dir();
    let new_path = std::path::PathBuf::from(&new_path);

    // 路径相同时不进行迁移
    if old_path == new_path {
        return Err("Source and destination paths are the same".to_string());
    }

    // 执行迁移
    let result = config::migrate_data(&old_path, &new_path)?;

    // 迁移成功后更新配置
    if result.success() {
        let mut new_config = AppConfig::load();
        new_config.data_path = Some(new_path.to_string_lossy().to_string());
        new_config.save()?;
    }

    Ok(result)
}

/// Tauri 命令：导出数据为 ZIP 文件
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

    // 使用 SQLite backup API 创建数据库的完整副本（包含所有 WAL 数据）
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

    // 弹出保存对话框
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

    // 打包 images 目录
    add_dir_to_zip(&mut zip, &data_dir.join("images"), "images", options)?;

    // 打包 icons 目录
    add_dir_to_zip(&mut zip, &data_dir.join("icons"), "icons", options)?;

    zip.finish().map_err(|e| e.to_string())?;

    let size = fs::metadata(&dest_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(format!("导出成功 ({})", format_size(size)))
}

/// Tauri 命令：从 ZIP 文件导入数据
#[tauri::command]
async fn import_data(app: tauri::AppHandle) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Read;
    use tauri_plugin_dialog::DialogExt;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    // 弹出打开对话框
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

    // 校验 ZIP 内容：必须包含 clipboard.db
    let has_db = (0..archive.len()).any(|i| {
        archive
            .by_index(i)
            .map(|f| f.name() == "clipboard.db")
            .unwrap_or(false)
    });
    if !has_db {
        return Err("ZIP 文件中未找到 clipboard.db，不是有效的备份文件".to_string());
    }

    // 确保数据目录存在
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

/// 检测并应用待导入的 staging 数据库文件
///
/// import_data 将数据库写为 `clipboard.db.import`（而非直接覆盖正在使用的数据库），
/// 启动时在打开数据库前调用此函数，删除旧 db/wal/shm 并将 staging 文件重命名为正式数据库。
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
        // 删除旧数据库及 WAL/SHM
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

    // 最后手段：复制替代重命名
    tracing::error!("Rename failed after 10 attempts, trying copy fallback");
    if fs::copy(&staging, db_path).is_ok() {
        let _ = fs::remove_file(&staging);
        tracing::info!("Import applied via copy fallback");
    } else {
        tracing::error!("Import staging completely failed");
    }
}

/// 将目录下所有文件添加到 ZIP
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

/// 生成时间戳字符串 (yyyyMMdd_HHmmss)
fn chrono_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 简单转换为本地时间格式
    let secs = now + 8 * 3600; // UTC+8 粗略偏移
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    // 从 Unix 天数计算年月日
    let (y, mo, d) = days_to_ymd(days);
    format!("{:04}{:02}{:02}_{:02}{:02}{:02}", y, mo, d, h, m, s)
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // 基于 Unix 纪元的简单日历计算
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

/// Tauri 命令：重启应用（支持 UAC 提权）
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    // 使用支持 UAC 的自定义重启
    if admin_launch::restart_app() {
        // 新实例已启动，退出当前进程
        app.exit(0);
    } else {
        // 回退到 Tauri 默认重启
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
            // 读取设置并恢复窗口尺寸
            let follow_cursor = app
                .try_state::<std::sync::Arc<commands::AppState>>()
                .map(|state| {
                    let repo = database::SettingsRepository::new(&state.db);
                    // 恢复持久化的窗口尺寸
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

            // 保存当前前台窗口，搜索框聚焦后需要还原
            input_monitor::save_current_focus();
            // 键盘导航开启且未固定时，窗口显示时直接可聚焦
            if input_monitor::is_keyboard_nav_enabled() && !input_monitor::is_window_pinned() {
                let _ = window.set_focusable(true);
            }
            let _ = window.show();
            positioning::force_topmost(&window);
            // 可聚焦模式下设置窗口焦点
            if input_monitor::is_keyboard_nav_enabled() && !input_monitor::is_window_pinned() {
                let _ = window.set_focus();
            }
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
            input_monitor::enable_mouse_monitoring();
            let _ = window.emit("window-shown", ());
        }
    }
}

/// Tauri 命令：启用 Win+V 替换（注册表禁用系统 Win+V，接管为应用快捷键）
#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // 记住当前快捷键以便失败时还原
    let saved_shortcut_str = get_current_shortcut();
    let saved_shortcut = parse_shortcut(&saved_shortcut_str);

    // 注销当前自定义快捷键
    if let Some(shortcut) = saved_shortcut {
        let _ = app.global_shortcut().unregister(shortcut);
    }

    // 通过注册表禁用系统 Win+V（重启 Explorer 生效）
    if let Err(e) = win_v_registry::disable_win_v_hotkey(true) {
        // 失败时重新注册自定义快捷键
        if let Some(sc) = saved_shortcut {
            let _ = app.global_shortcut().on_shortcut(sc, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
        }
        return Err(e);
    }
    // 接管 Win+V：系统已禁用，通过 global_shortcut 注册
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    if let Err(e) = app.global_shortcut()
        .on_shortcut(winv_shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
    {
        // 还原：重新启用系统 Win+V 并注册自定义快捷键
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

    // 保存设置
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "true");
    Ok(())
}

/// Tauri 命令：禁用 Win+V 替换（恢复系统 Win+V 与自定义快捷键）
#[tauri::command]
async fn disable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // 注销 Win+V 快捷键
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    let _ = app.global_shortcut().unregister(winv_shortcut);

    // 通过注册表重新启用系统 Win+V（重启 Explorer 生效）
    win_v_registry::enable_win_v_hotkey(true)?;

    // 重新注册自定义快捷键
    if let Some(shortcut) = parse_shortcut(&get_current_shortcut()) {
        let _ = app
            .global_shortcut()
            .on_shortcut(shortcut, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });
    }

    // 保存设置
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "false");
    Ok(())
}

/// Tauri 命令：检查 Win+V 替换是否已启用
#[tauri::command]
async fn is_winv_replacement_enabled(_app: tauri::AppHandle) -> bool {
    // 检查注册表状态
    win_v_registry::is_win_v_hotkey_disabled()
}

/// Tauri 命令：更新主快捷键
#[tauri::command]
async fn update_shortcut(app: tauri::AppHandle, new_shortcut: String) -> Result<String, String> {
    // 解析新快捷键
    let new_sc = parse_shortcut(&new_shortcut)
        .ok_or_else(|| format!("Invalid shortcut: {}", new_shortcut))?;

    if !shortcut_has_modifier(&new_shortcut) {
        return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
    }

    // 注销当前快捷键
    if let Some(current_sc) = parse_shortcut(&get_current_shortcut()) {
        let _ = app.global_shortcut().unregister(current_sc);
    }

    // 注册新快捷键（绑定窗口切换）
    app.global_shortcut()
        .on_shortcut(new_sc, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // 更新全局状态
    *CURRENT_SHORTCUT.write() = Some(new_shortcut.clone());

    Ok(new_shortcut)
}

/// Tauri 命令：获取当前快捷键
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

/// Tauri 命令：获取快速粘贴快捷键（槽位 1-9）
#[tauri::command]
fn get_quick_paste_shortcuts() -> Vec<String> {
    let current = CURRENT_QUICK_PASTE_SHORTCUTS.read();
    if current.len() == 9 {
        return current.clone();
    }
    default_quick_paste_shortcuts()
}

/// Tauri 命令：更新指定槽位（1-9）的快速粘贴快捷键
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
    // 检查与主快捷键是否冲突
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
        // 仅在该槽位注册失败时回滚
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

/// Tauri 命令：设置窗口固定状态
/// 固定时切换为不可聚焦（点哪儿粘哪儿），取消固定时根据键盘导航设置恢复
#[tauri::command]
async fn set_window_pinned(window: tauri::WebviewWindow, pinned: bool) {
    input_monitor::set_window_pinned(pinned);
    if pinned {
        // 固定模式：不可聚焦，还原目标窗口焦点
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
    } else if input_monitor::is_keyboard_nav_enabled() {
        // 取消固定 + 键盘导航开启：恢复可聚焦
        input_monitor::save_current_focus();
        let _ = window.set_focusable(true);
        let _ = window.set_focus();
    }
}

/// Tauri 命令：获取窗口固定状态
#[tauri::command]
fn is_window_pinned() -> bool {
    input_monitor::is_window_pinned()
}

/// Tauri 命令：临时启用窗口焦点（搜索框聚焦时调用）
#[tauri::command]
async fn focus_clipboard_window(window: tauri::WebviewWindow) {
    input_monitor::focus_clipboard_window(&window);
}

/// Tauri 命令：恢复非聚焦模式并还原目标窗口焦点（搜索框失焦时调用）
#[tauri::command]
async fn restore_last_focus(window: tauri::WebviewWindow) {
    input_monitor::restore_last_focus(&window);
}

/// Tauri 命令：仅保存当前前台窗口句柄（窗口显示时调用）
#[tauri::command]
fn save_current_focus() {
    input_monitor::save_current_focus();
}

/// Tauri 命令：设置键盘导航状态
#[tauri::command]
async fn set_keyboard_nav_enabled(window: tauri::WebviewWindow, enabled: bool) {
    input_monitor::set_keyboard_nav_enabled(enabled);
    // 如果窗口可见且未固定，立即更新可聚焦状态
    if window.is_visible().unwrap_or(false) && !input_monitor::is_window_pinned() {
        if enabled {
            input_monitor::save_current_focus();
            let _ = window.set_focusable(true);
            let _ = window.set_focus();
        } else {
            input_monitor::restore_last_focus(&window);
        }
    }
}

/// Tauri 命令：检查是否启用管理员启动
#[tauri::command]
fn is_admin_launch_enabled() -> bool {
    admin_launch::is_admin_launch_enabled()
}

/// Tauri 命令：启用管理员启动
#[tauri::command]
fn enable_admin_launch() -> Result<(), String> {
    admin_launch::enable_admin_launch()
}

/// Tauri 命令：禁用管理员启动
#[tauri::command]
fn disable_admin_launch() -> Result<(), String> {
    admin_launch::disable_admin_launch()
}

/// Tauri 命令：检查当前是否以管理员身份运行
#[tauri::command]
fn is_running_as_admin() -> bool {
    admin_launch::is_running_as_admin()
}

// ============ 更新命令 ============

/// Tauri 命令：检查 GitHub 更新
#[tauri::command]
async fn check_for_update() -> Result<updater::UpdateInfo, String> {
    tokio::task::spawn_blocking(updater::check_update)
        .await
        .map_err(|e| e.to_string())?
}

/// Tauri 命令：下载更新安装包（带进度事件）
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

/// Tauri 命令：取消正在进行的下载
#[tauri::command]
fn cancel_update_download() {
    updater::cancel_download();
}

/// Tauri 命令：启动安装程序并退出应用
#[tauri::command]
async fn install_update(app: tauri::AppHandle, installer_path: String) -> Result<(), String> {
    updater::install(&installer_path)?;
    // 等待安装程序启动后再退出
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    app.exit(0);
    Ok(())
}

// ============ 图片预览窗口 ============

/// Tauri 命令：在固定尺寸透明窗口中显示图片预览，图片缩放由 webview CSS 处理。
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

    // 记录 WebView2 运行时版本（通过 wry API 查询，不依赖注册表路径）
    match wry::webview_version() {
        Ok(ver) => tracing::info!("WebView2 runtime version: {}", ver),
        Err(e) => tracing::warn!("WebView2 version query failed: {}", e),
    }

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

            // 检测并应用待导入的 staging 文件（由 import_data 写入）
            apply_pending_import(&db_path);

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
                // 恢复持久化的窗口尺寸（仅在启用时）
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

                // 记录 DPI 感知状态（Tauri 初始化后才有意义）
                #[cfg(target_os = "windows")]
                {
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
            focus_clipboard_window,
            restore_last_focus,
            save_current_focus,
            set_keyboard_nav_enabled,
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
            cancel_update_download,
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
