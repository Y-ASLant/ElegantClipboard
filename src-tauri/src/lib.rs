mod admin_launch;
mod clipboard;
mod commands;
mod config;
mod database;
mod game_mode;
mod input_monitor;
mod keyboard_hook;
mod positioning;
mod hotkey;
mod low_level_shortcut;
mod shortcut;
mod task_scheduler;
mod tray;
mod updater;
pub(crate) mod proxy;
mod webdav;
mod win_v_registry;

use clipboard::ClipboardMonitor;
use commands::AppState;
use config::AppConfig;
use database::Database;
use database::SettingsRepository;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::Manager;
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
static CURRENT_FAVORITE_PASTE_SHORTCUTS: parking_lot::RwLock<Vec<String>> =
    parking_lot::RwLock::new(Vec::new());
static QUICK_PASTE_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
static ACTIVE_QUICK_PASTE_SLOTS: std::sync::LazyLock<parking_lot::Mutex<HashSet<u8>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashSet::new()));
static ACTIVE_FAVORITE_PASTE_SLOTS: std::sync::LazyLock<parking_lot::Mutex<HashSet<u8>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashSet::new()));
/// simulate_paste 释放修饰键时可能导致 OS 重新触发快捷键，用此标志拦截假触发
static PASTE_IN_PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// 快捷键是否已被用户临时禁用（Win+V 除外）
pub(crate) static SHORTCUTS_DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy)]
enum PasteKind {
    Quick,
    Favorite,
}

/// 全局快捷键回调：按下时切换窗口可见性
fn on_toggle_shortcut(app: &tauri::AppHandle, state: hotkey::KeyState) {
    if state == hotkey::KeyState::Pressed {
        commands::window::toggle_window_visibility(app, true);
    }
}

impl PasteKind {
    fn label(self) -> &'static str {
        match self {
            PasteKind::Quick => "槽位",
            PasteKind::Favorite => "收藏槽位",
        }
    }
    fn defaults(self) -> Vec<String> {
        match self {
            PasteKind::Quick => default_quick_paste_shortcuts(),
            PasteKind::Favorite => default_favorite_paste_shortcuts(),
        }
    }
    fn setting_key(self, slot: u8) -> String {
        match self {
            PasteKind::Quick => quick_paste_setting_key(slot),
            PasteKind::Favorite => favorite_paste_setting_key(slot),
        }
    }
    fn read_current(self) -> Vec<String> {
        let current = match self {
            PasteKind::Quick => CURRENT_QUICK_PASTE_SHORTCUTS.read().clone(),
            PasteKind::Favorite => CURRENT_FAVORITE_PASTE_SHORTCUTS.read().clone(),
        };
        if current.len() == 9 { current } else { self.defaults() }
    }
}

/// 注销一组快捷键（含小键盘变体）
fn unregister_shortcut_list(list: &[String]) {
    for s in list {
        if s.is_empty() { continue; }
        hotkey::unregister(s);
        if let Some(numpad_str) = numpad_variant_str(s) {
            hotkey::unregister(&numpad_str);
        }
    }
}

/// 注销所有快捷键（Win+V 除外）
pub(crate) fn disable_all_shortcuts(app: &tauri::AppHandle) {
    hotkey::unregister(&get_current_shortcut());
    unregister_shortcut_list(&CURRENT_QUICK_PASTE_SHORTCUTS.read());
    unregister_shortcut_list(&CURRENT_FAVORITE_PASTE_SHORTCUTS.read());
    commands::ocr::unregister_ocr_shortcut(app);
    commands::translate::unregister_translate_selection_shortcut(app);
}

/// 重新注册所有快捷键（根据 Win+V 替代状态自动选择主呼出键）
pub(crate) fn enable_all_shortcuts(app: &tauri::AppHandle) {
    // Win+V 替代模式下只注册 Win+V，否则注册自定义快捷键
    if win_v_registry::is_win_v_hotkey_disabled() {
        hotkey::register("Win+V", Arc::new(on_toggle_shortcut));
    } else {
        hotkey::register(&get_current_shortcut(), Arc::new(on_toggle_shortcut));
    }
    let shortcuts = CURRENT_QUICK_PASTE_SHORTCUTS.read().clone();
    apply_paste_shortcuts(app, &shortcuts, PasteKind::Quick);
    let fav_shortcuts = CURRENT_FAVORITE_PASTE_SHORTCUTS.read().clone();
    apply_paste_shortcuts(app, &fav_shortcuts, PasteKind::Favorite);
    commands::ocr::register_ocr_shortcut(app);
    commands::translate::register_translate_selection_shortcut(app);
}

/// 临时禁用所有快捷键（Win+V 除外），返回切换后的禁用状态
pub fn toggle_shortcuts_disabled(app: &tauri::AppHandle) -> bool {
    use std::sync::atomic::Ordering;
    let was = SHORTCUTS_DISABLED.fetch_xor(true, Ordering::SeqCst);
    let disabled = !was;
    if disabled {
        disable_all_shortcuts(app);
        tracing::info!("All shortcuts disabled (except Win+V)");
    } else {
        enable_all_shortcuts(app);
        tracing::info!("All shortcuts re-enabled");
    }
    disabled
}

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

fn default_favorite_paste_shortcuts() -> Vec<String> {
    // 默认只有前 3 个槽位有快捷键，其余留空
    let mut shortcuts = vec![String::new(); 9];
    shortcuts[0] = "Ctrl+Alt+1".to_string();
    shortcuts[1] = "Ctrl+Alt+2".to_string();
    shortcuts[2] = "Ctrl+Alt+3".to_string();
    shortcuts
}

fn favorite_paste_setting_key(slot: u8) -> String {
    format!("favorite_paste_shortcut_{}", slot)
}

fn load_favorite_paste_shortcuts(repo: &SettingsRepository) -> Vec<String> {
    let mut shortcuts = default_favorite_paste_shortcuts();
    for slot in 1..=9 {
        let key = favorite_paste_setting_key(slot);
        if let Ok(Some(value)) = repo.get(&key) {
            shortcuts[(slot - 1) as usize] = normalize_shortcut_value(&value);
        }
    }
    shortcuts
}

/// 若快捷键的主键是数字（0-9），返回对应的小键盘变体字符串，如 "Alt+1" → "Alt+Numpad1"
fn numpad_variant_str(shortcut_str: &str) -> Option<String> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    let last = *parts.last()?;
    if last.len() == 1 && last.chars().next()?.is_ascii_digit() {
        let mut result = parts[..parts.len() - 1].join("+");
        if !result.is_empty() {
            result.push('+');
        }
        result.push_str(&format!("Numpad{}", last));
        Some(result)
    } else {
        None
    }
}

fn apply_paste_shortcuts(
    _app: &tauri::AppHandle,
    shortcuts: &[String],
    kind: PasteKind,
) -> HashMap<u8, String> {
    // 注销旧快捷键
    let old = match kind {
        PasteKind::Quick => CURRENT_QUICK_PASTE_SHORTCUTS.read().clone(),
        PasteKind::Favorite => CURRENT_FAVORITE_PASTE_SHORTCUTS.read().clone(),
    };
    unregister_shortcut_list(&old);

    let label = kind.label();
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

        // 验证快捷键格式是否有效
        if !hotkey::register(&normalized, make_paste_handler(slot, kind)) {
            failures.insert(slot, format!("{} {} 快捷键格式无效: {}", label, slot, normalized));
            continue;
        }

        // 自动为数字键注册小键盘变体
        if let Some(numpad_str) = numpad_variant_str(&normalized) {
            hotkey::register(&numpad_str, make_paste_handler(slot, kind));
        }
    }

    match kind {
        PasteKind::Quick => *CURRENT_QUICK_PASTE_SHORTCUTS.write() = applied,
        PasteKind::Favorite => *CURRENT_FAVORITE_PASTE_SHORTCUTS.write() = applied,
    }
    failures
}

fn make_paste_handler(slot: u8, kind: PasteKind) -> hotkey::ShortcutCallback {
    Arc::new(move |app: &tauri::AppHandle, key_state: hotkey::KeyState| {
        match key_state {
            hotkey::KeyState::Pressed => {
                let any_focused = app
                    .webview_windows()
                    .values()
                    .any(|w| w.is_focused().unwrap_or(false));
                if any_focused { return; }
                if PASTE_IN_PROGRESS.load(std::sync::atomic::Ordering::Acquire) { return; }
                let active_slots = match kind {
                    PasteKind::Quick => &*ACTIVE_QUICK_PASTE_SLOTS,
                    PasteKind::Favorite => &*ACTIVE_FAVORITE_PASTE_SLOTS,
                };
                let is_first = active_slots.lock().insert(slot);
                let state = app.state::<Arc<AppState>>().inner().clone();
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    let _guard = QUICK_PASTE_LOCK.lock();
                    PASTE_IN_PROGRESS.store(true, std::sync::atomic::Ordering::Release);
                    if is_first {
                        let result = match kind {
                            PasteKind::Quick => commands::clipboard::quick_paste_by_slot(&state, &app_handle, slot),
                            PasteKind::Favorite => commands::clipboard::quick_paste_favorite_by_slot(&state, &app_handle, slot),
                        };
                        if let Err(err) = result {
                            tracing::warn!("{} {} 粘贴失败: {}", kind.label(), slot, err);
                            active_slots.lock().remove(&slot);
                        }
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if let Err(err) = commands::clipboard::simulate_paste() {
                            tracing::warn!("{} {} 重复粘贴失败: {}", kind.label(), slot, err);
                        }
                    }
                    PASTE_IN_PROGRESS.store(false, std::sync::atomic::Ordering::Release);
                });
            }
            hotkey::KeyState::Released => {
                match kind {
                    PasteKind::Quick => ACTIVE_QUICK_PASTE_SLOTS.lock().remove(&slot),
                    PasteKind::Favorite => ACTIVE_FAVORITE_PASTE_SLOTS.lock().remove(&slot),
                };
            }
        }
    })
}

static FILE_LOG_GUARD: parking_lot::Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> =
    parking_lot::Mutex::new(None);

fn rotate_log_if_needed(log_path: &std::path::Path, max_size: u64) {
    if let Ok(meta) = std::fs::metadata(log_path)
        && meta.len() > max_size {
            let backup = log_path.with_extension("log.old");
            let _ = std::fs::rename(log_path, backup);
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


/// 根据用户设置应用托盘图标可见性。
/// 托盘图标始终存在（仅创建一次），通过 set_visible 切换显隐。
pub(crate) fn restore_tray_visibility(app: &tauri::AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let show_tray = settings_repo.get_bool("show_tray_icon", true);
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(show_tray);
    }
}

#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    let saved_shortcut_str = get_current_shortcut();
    hotkey::unregister(&saved_shortcut_str);

    if let Err(e) = win_v_registry::disable_win_v_hotkey(true) {
        hotkey::register(&saved_shortcut_str, Arc::new(on_toggle_shortcut));
        return Err(e);
    }
    if !hotkey::register("Win+V", Arc::new(on_toggle_shortcut)) {
        let _ = win_v_registry::enable_win_v_hotkey(true);
        hotkey::register(&saved_shortcut_str, Arc::new(on_toggle_shortcut));
        return Err("Failed to register Win+V shortcut".to_string());
    }

    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    let _ = settings_repo.set("winv_replacement", "true");
    Ok(())
}

#[tauri::command]
async fn disable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    hotkey::unregister("Win+V");

    win_v_registry::enable_win_v_hotkey(true)?;

    hotkey::register(&get_current_shortcut(), Arc::new(on_toggle_shortcut));

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
async fn update_shortcut(_app: tauri::AppHandle, new_shortcut: String) -> Result<String, String> {
    if !shortcut_has_modifier(&new_shortcut) {
        return Err("快捷键至少包含一个修饰键 (Ctrl/Alt/Win)".to_string());
    }

    hotkey::unregister(&get_current_shortcut());

    if !hotkey::register(&new_shortcut, Arc::new(on_toggle_shortcut)) {
        // 注册失败，恢复旧快捷键
        hotkey::register(&get_current_shortcut(), Arc::new(on_toggle_shortcut));
        return Err(format!("Invalid shortcut: {}", new_shortcut));
    }

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

fn reload_paste_shortcuts_from_settings(app: &tauri::AppHandle, kind: PasteKind) -> HashMap<u8, String> {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = SettingsRepository::new(&state.db);
    let shortcuts = match kind {
        PasteKind::Quick => load_quick_paste_shortcuts(&settings_repo),
        PasteKind::Favorite => load_favorite_paste_shortcuts(&settings_repo),
    };
    apply_paste_shortcuts(app, &shortcuts, kind)
}

fn set_paste_shortcut_inner(
    app: &tauri::AppHandle,
    slot: u8,
    shortcut: String,
    kind: PasteKind,
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
        if !shortcut_has_modifier(&normalized) {
            return Err("快捷键至少包含一个修饰键 (Ctrl/Alt)".to_string());
        }
        let main_sc = get_current_shortcut();
        if normalized.eq_ignore_ascii_case(&main_sc) {
            return Err(format!("与呼出快捷键 {} 冲突", main_sc));
        }
    }

    let mut next_shortcuts = kind.read_current();
    let idx = (slot - 1) as usize;
    let previous = next_shortcuts[idx].clone();
    next_shortcuts[idx] = normalized.clone();

    let failures = apply_paste_shortcuts(app, &next_shortcuts, kind);
    if let Some(err) = failures.get(&slot) {
        next_shortcuts[idx] = previous;
        let _ = apply_paste_shortcuts(app, &next_shortcuts, kind);
        return Err(err.clone());
    }

    let state = app.state::<Arc<AppState>>();
    let settings_repo = SettingsRepository::new(&state.db);
    settings_repo
        .set(&kind.setting_key(slot), &normalized)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_quick_paste_shortcuts() -> Vec<String> {
    PasteKind::Quick.read_current()
}

#[tauri::command]
fn set_quick_paste_shortcut(app: tauri::AppHandle, slot: u8, shortcut: String) -> Result<(), String> {
    set_paste_shortcut_inner(&app, slot, shortcut, PasteKind::Quick)
}

#[tauri::command]
fn get_favorite_paste_shortcuts() -> Vec<String> {
    PasteKind::Favorite.read_current()
}

#[tauri::command]
fn set_favorite_paste_shortcut(app: tauri::AppHandle, slot: u8, shortcut: String) -> Result<(), String> {
    set_paste_shortcut_inner(&app, slot, shortcut, PasteKind::Favorite)
}

/// 云端同步下载后，重新加载运行时设置（快捷键、托盘图标、游戏模式等）
#[tauri::command]
fn reload_runtime_settings(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);

    // 0. 同步热键注册模式（云端设置可能与本地不同）
    let saved_mode = hotkey::HotkeyMode::from_str(&settings_repo.get_or("hotkey_mode", "register"));
    if hotkey::get_mode() != saved_mode {
        disable_all_shortcuts(&app);
        if win_v_registry::is_win_v_hotkey_disabled() {
            hotkey::unregister("Win+V");
        }
        hotkey::switch_mode(&app, saved_mode);
        tracing::info!("热键模式已切换为: {:?}（云端同步）", saved_mode);
    }

    // 1. 重新注册主呼出快捷键
    disable_all_shortcuts(&app);
    let saved_shortcut = settings_repo.get_or("toggle_shortcut", "Alt+C");
    *CURRENT_SHORTCUT.write() = Some(saved_shortcut.clone());

    let shortcut_str = if win_v_registry::is_win_v_hotkey_disabled() {
        "Win+V".to_string()
    } else {
        saved_shortcut
    };
    hotkey::register(&shortcut_str, Arc::new(on_toggle_shortcut));

    // 2. 重新注册快速粘贴 / 收藏粘贴快捷键
    for kind in [PasteKind::Quick, PasteKind::Favorite] {
        reload_paste_shortcuts_from_settings(&app, kind);
    }

    // 3. 重新注册 OCR 快捷键
    commands::ocr::register_ocr_shortcut(&app);

    // 4. 重新注册翻译选中文字快捷键
    commands::translate::register_translate_selection_shortcut(&app);

    // 5. 刷新托盘图标可见性
    restore_tray_visibility(&app);

    // 6. 刷新游戏模式
    let game_mode = settings_repo.get_bool("game_mode_enabled", false);
    if game_mode {
        game_mode::start(app.clone());
    } else {
        game_mode::stop();
    }

    tracing::info!("运行时设置已重新加载（云端同步后）");
    Ok(())
}

#[tauri::command]
fn set_game_mode_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    settings_repo
        .set("game_mode_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| format!("保存游戏模式设置失败: {}", e))?;

    if enabled {
        game_mode::start(app);
    } else {
        game_mode::stop();
    }
    Ok(())
}

#[tauri::command]
fn is_game_mode_enabled(app: tauri::AppHandle) -> bool {
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    settings_repo.get_bool("game_mode_enabled", false)
}

#[tauri::command]
fn get_hotkey_mode() -> String {
    hotkey::get_mode().as_str().to_string()
}

#[tauri::command]
fn set_hotkey_mode(app: tauri::AppHandle, mode: String) -> Result<(), String> {
    let new_mode = hotkey::HotkeyMode::from_str(&mode);
    let old_mode = hotkey::get_mode();
    if old_mode == new_mode {
        return Ok(());
    }

    // 注销所有快捷键
    disable_all_shortcuts(&app);
    if win_v_registry::is_win_v_hotkey_disabled() {
        hotkey::unregister("Win+V");
    }

    // 切换模式
    hotkey::switch_mode(&app, new_mode);

    // 重新注册所有快捷键
    enable_all_shortcuts(&app);

    // 保存设置
    let state = app.state::<Arc<AppState>>();
    let settings_repo = database::SettingsRepository::new(&state.db);
    settings_repo
        .set("hotkey_mode", new_mode.as_str())
        .map_err(|e| format!("保存热键模式失败: {}", e))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = AppConfig::load();
    init_logging(&config);

    match tauri::webview_version() {
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
        .setup(move |app| {
            let db_path = config.get_db_path();
            let images_path = config.get_images_path();

            commands::data_transfer::apply_pending_import(&db_path);

            let db = Database::new(db_path).map_err(|e| e.to_string())?;

            // 清理孤立的图片/图标文件（磁盘有文件但数据库无引用）
            clipboard::cleanup_orphan_files(&db, &config.get_data_dir());

            let monitor = ClipboardMonitor::new();
            monitor.init(&db, images_path);

            let state = Arc::new(AppState { db, monitor });

            let settings_repo = database::SettingsRepository::new(&state.db);

            // 安装更新后注册表 Run 条目会被清除，根据数据库偏好自动恢复自启动
            {
                use tauri_plugin_autostart::ManagerExt;
                let want_autostart = settings_repo.get_bool("autostart_enabled", false);
                if want_autostart {
                    match app.autolaunch().is_enabled() {
                        Ok(false) => {
                            if let Err(e) = app.autolaunch().enable() {
                                tracing::warn!("自启动恢复失败: {}", e);
                            } else {
                                tracing::info!("自启动已自动恢复（更新/导入后）");
                            }
                        }
                        Err(e) => tracing::warn!("检查自启动状态失败: {}", e),
                        _ => {}
                    }
                }
            }

            let saved_shortcut = settings_repo.get_or("global_shortcut", "Alt+C");

            // 恢复窗口置顶状态
            let saved_pinned = settings_repo.get_bool("window_pinned", false);
            if saved_pinned {
                input_monitor::set_window_pinned(true);
            }

            state.monitor.start(app.handle().clone());
            app.manage(state);

            // 托盘图标始终创建（仅一次），通过 set_visible 控制显隐，
            // 避免反复创建/销毁导致 Explorer 重启时出现重复图标。
            let _ = tray::setup_tray(app.handle());
            restore_tray_visibility(app.handle());

            // 根据设置选择热键模式
            let hotkey_mode = hotkey::HotkeyMode::from_str(
                &settings_repo.get_or("hotkey_mode", "register"),
            );
            hotkey::start(app.handle().clone(), hotkey_mode);

            *CURRENT_SHORTCUT.write() = Some(saved_shortcut.clone());
            let shortcut_str = if win_v_registry::is_win_v_hotkey_disabled() {
                "Win+V".to_string()
            } else {
                saved_shortcut.clone()
            };
            hotkey::register(&shortcut_str, Arc::new(on_toggle_shortcut));

            for kind in [PasteKind::Quick, PasteKind::Favorite] {
                let failures = reload_paste_shortcuts_from_settings(app.handle(), kind);
                for (slot, err) in &failures {
                    tracing::warn!("{} {} 快捷键注册失败: {}", kind.label(), slot, err);
                }
            }

            // 注册 OCR 快捷键
            commands::ocr::register_ocr_shortcut(app.handle());

            // 注册翻译选中文字快捷键
            commands::translate::register_translate_selection_shortcut(app.handle());

            // 启动游戏模式检测
            let game_mode = settings_repo.get_bool("game_mode_enabled", false);
            if game_mode {
                game_mode::start(app.handle().clone());
            }

            if let Some(window) = app.get_webview_window("main") {
                let persist = settings_repo.get_bool("persist_window_size", true);
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
                // 启动阶段恢复「上一次位置」，覆盖托盘左键首次显示路径
                let position_mode = crate::positioning::PositionMode::from_str(
                    &settings_repo.get_or("position_mode", "follow_cursor"),
                );
                if position_mode == crate::positioning::PositionMode::FixedPosition {
                    let x = settings_repo
                        .get("window_x")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse::<i32>().ok());
                    let y = settings_repo
                        .get("window_y")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse::<i32>().ok());
                    if let (Some(x), Some(y)) = (x, y) {
                        let _ = window.set_position(tauri::Position::Physical(
                            tauri::PhysicalPosition::new(x, y),
                        ));
                    }
                }

                let _ = window.set_focusable(false);

                #[cfg(target_os = "windows")]
                {
                    // 启动时设置 WS_EX_LAYERED 确保窗口不透明，防止 Win10 无 DWM 特效时闪烁
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

            // 启动 WebDAV 自动同步后台线程
            {
                let app_state = app.state::<Arc<AppState>>();
                webdav::start_auto_sync_task(
                    app_state.db.clone(),
                    config.get_data_dir(),
                );
            }

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

            // 启动后 30 秒自动检查更新（可在设置中关闭）
            {
                let auto_check = settings_repo
                    .get("auto_check_update")
                    .ok()
                    .flatten()
                    .map(|v| v != "false")
                    .unwrap_or(true); // 默认开启
                if auto_check {
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        use tauri::Emitter;
                        use tauri_plugin_notification::NotificationExt;
                        std::thread::sleep(std::time::Duration::from_secs(30));
                        match updater::check_update() {
                            Ok(info) if info.has_update => {
                                tracing::info!(
                                    "Auto update check: new version v{} available",
                                    info.latest_version
                                );
                                let _ = app_handle
                                    .notification()
                                    .builder()
                                    .title("发现新版本")
                                    .body(format!(
                                        "v{} → v{}，可在设置中查看详情",
                                        info.current_version, info.latest_version
                                    ))
                                    .show();
                                let _ = app_handle.emit("auto-update-available", info);
                            }
                            Ok(_) => {
                                tracing::info!("Auto update check: already at latest version");
                            }
                            Err(e) => {
                                tracing::warn!("Auto update check failed: {}", e);
                            }
                        }
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::preview::get_app_version,
            commands::data_transfer::get_default_data_path,
            commands::data_transfer::get_original_default_path,
            commands::data_transfer::check_path_has_data,
            commands::data_transfer::cleanup_data_at_path,
            commands::data_transfer::set_data_path,
            commands::data_transfer::migrate_data_to_path,
            commands::data_transfer::export_data,
            commands::data_transfer::import_data,
            commands::data_transfer::restart_app,
            commands::window::show_window,
            commands::window::hide_window,
            commands::window::set_window_visibility,
            commands::window::minimize_window,
            commands::window::toggle_maximize,
            commands::window::close_window,
            commands::preview::open_settings_window,
            commands::preview::show_image_preview,
            commands::preview::hide_image_preview,
            commands::preview::show_text_preview,
            commands::preview::hide_text_preview,
            commands::preview::open_text_editor_window,
            commands::window::set_window_pinned,
            commands::window::is_window_pinned,
            commands::window::set_window_effect,
            commands::window::focus_clipboard_window,
            commands::window::restore_last_focus,
            commands::window::save_current_focus,
            commands::window::set_keyboard_nav_enabled,
            commands::window::is_admin_launch_enabled,
            commands::window::enable_admin_launch,
            commands::window::disable_admin_launch,
            commands::window::is_running_as_admin,
            commands::window::is_windows_11,
            commands::preview::is_log_to_file_enabled,
            commands::preview::set_log_to_file,
            commands::preview::get_log_file_path,
            enable_winv_replacement,
            disable_winv_replacement,
            is_winv_replacement_enabled,
            update_shortcut,
            get_current_shortcut,
            get_quick_paste_shortcuts,
            set_quick_paste_shortcut,
            get_favorite_paste_shortcuts,
            set_favorite_paste_shortcut,
            commands::window::check_for_update,
            commands::window::download_update,
            commands::window::cancel_update_download,
            commands::window::install_update,
            reload_runtime_settings,
            set_game_mode_enabled,
            is_game_mode_enabled,
            get_hotkey_mode,
            set_hotkey_mode,
            commands::clipboard::get_clipboard_items,
            commands::clipboard::get_clipboard_item,
            commands::clipboard::get_clipboard_count,
            commands::clipboard::toggle_pin,
            commands::clipboard::toggle_favorite,
            commands::clipboard::move_clipboard_item,
            commands::clipboard::bump_item_to_top,
            commands::clipboard::delete_clipboard_item,
            commands::clipboard::batch_delete_clipboard_items,
            commands::clipboard::clear_history,
            commands::clipboard::clear_all_history,
            commands::clipboard::copy_to_clipboard,
            commands::clipboard::paste_content,
            commands::clipboard::paste_content_as_plain,
            commands::clipboard::paste_text_direct,
            commands::clipboard::merge_paste_content,
            commands::clipboard::update_text_content,
            commands::settings::get_running_apps,
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
            commands::settings::is_portable_mode,
            commands::settings::is_autostart_enabled,
            commands::settings::enable_autostart,
            commands::settings::disable_autostart,
            commands::settings::get_system_accent_color,
            commands::settings::get_system_fonts,
            commands::settings::set_tray_visible,
            commands::file_ops::check_files_exist,
            commands::file_ops::refresh_files_validity,
            commands::file_ops::show_in_explorer,
            commands::file_ops::paste_as_path,
            commands::file_ops::get_file_details,
            commands::file_ops::save_file_as,
            commands::file_ops::get_data_size,
            commands::tags::get_tags,
            commands::tags::create_tag,
            commands::tags::rename_tag,
            commands::tags::delete_tag,
            commands::tags::add_tag_to_item,
            commands::tags::remove_tag_from_item,
            commands::tags::get_item_tags,
            commands::tags::reorder_tags,
            commands::tags::reorder_tag_items,
            commands::sync::webdav_test_connection,
            commands::sync::webdav_upload,
            commands::sync::webdav_download,
            commands::translate::translate_text,
            commands::translate::write_text_to_clipboard,
            commands::translate::get_pending_translate_text,
            commands::translate::open_translate_result_window,
            commands::translate::update_translate_selection_shortcut,
            commands::ocr::ocr_capture_screen,
            commands::ocr::ocr_crop_region,
            commands::ocr::ocr_recognize_baidu,
            commands::ocr::open_ocr_screenshot_window,
            commands::ocr::ocr_screenshot_ready,
            commands::ocr::open_ocr_result_window,
            commands::ocr::get_pending_ocr_text,
            commands::ocr::update_ocr_shortcut,
            commands::ocr::ocr_toggle_enabled,
            commands::tts::tts_speak_edge,
            commands::tts::tts_get_edge_voices,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = run_result {
        eprintln!("error while running tauri application: {err}");
    }
}
