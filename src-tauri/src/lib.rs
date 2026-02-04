mod clipboard;
mod commands;
mod config;
mod database;
mod input_monitor;
mod keyboard_hook;
mod tray;
mod win_v_registry;

use clipboard::ClipboardMonitor;
use commands::AppState;
use config::AppConfig;
use database::Database;
use std::sync::{Arc, RwLock};
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

/// Global state for current shortcut
static CURRENT_SHORTCUT: RwLock<Option<String>> = RwLock::new(None);

/// Parse shortcut string to Shortcut object
fn parse_shortcut(shortcut_str: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let mut key_code = None;

    for part in parts {
        match part.to_uppercase().as_str() {
            "CTRL" | "CONTROL" => modifiers |= Modifiers::CONTROL,
            "ALT" => modifiers |= Modifiers::ALT,
            "SHIFT" => modifiers |= Modifiers::SHIFT,
            "WIN" | "SUPER" | "META" | "CMD" => modifiers |= Modifiers::SUPER,
            // Letters
            "A" => key_code = Some(Code::KeyA),
            "B" => key_code = Some(Code::KeyB),
            "C" => key_code = Some(Code::KeyC),
            "D" => key_code = Some(Code::KeyD),
            "E" => key_code = Some(Code::KeyE),
            "F" => key_code = Some(Code::KeyF),
            "G" => key_code = Some(Code::KeyG),
            "H" => key_code = Some(Code::KeyH),
            "I" => key_code = Some(Code::KeyI),
            "J" => key_code = Some(Code::KeyJ),
            "K" => key_code = Some(Code::KeyK),
            "L" => key_code = Some(Code::KeyL),
            "M" => key_code = Some(Code::KeyM),
            "N" => key_code = Some(Code::KeyN),
            "O" => key_code = Some(Code::KeyO),
            "P" => key_code = Some(Code::KeyP),
            "Q" => key_code = Some(Code::KeyQ),
            "R" => key_code = Some(Code::KeyR),
            "S" => key_code = Some(Code::KeyS),
            "T" => key_code = Some(Code::KeyT),
            "U" => key_code = Some(Code::KeyU),
            "V" => key_code = Some(Code::KeyV),
            "W" => key_code = Some(Code::KeyW),
            "X" => key_code = Some(Code::KeyX),
            "Y" => key_code = Some(Code::KeyY),
            "Z" => key_code = Some(Code::KeyZ),
            // Numbers
            "0" => key_code = Some(Code::Digit0),
            "1" => key_code = Some(Code::Digit1),
            "2" => key_code = Some(Code::Digit2),
            "3" => key_code = Some(Code::Digit3),
            "4" => key_code = Some(Code::Digit4),
            "5" => key_code = Some(Code::Digit5),
            "6" => key_code = Some(Code::Digit6),
            "7" => key_code = Some(Code::Digit7),
            "8" => key_code = Some(Code::Digit8),
            "9" => key_code = Some(Code::Digit9),
            // Function keys
            "F1" => key_code = Some(Code::F1),
            "F2" => key_code = Some(Code::F2),
            "F3" => key_code = Some(Code::F3),
            "F4" => key_code = Some(Code::F4),
            "F5" => key_code = Some(Code::F5),
            "F6" => key_code = Some(Code::F6),
            "F7" => key_code = Some(Code::F7),
            "F8" => key_code = Some(Code::F8),
            "F9" => key_code = Some(Code::F9),
            "F10" => key_code = Some(Code::F10),
            "F11" => key_code = Some(Code::F11),
            "F12" => key_code = Some(Code::F12),
            // Special keys
            "SPACE" => key_code = Some(Code::Space),
            "TAB" => key_code = Some(Code::Tab),
            "ENTER" | "RETURN" => key_code = Some(Code::Enter),
            "BACKSPACE" => key_code = Some(Code::Backspace),
            "DELETE" | "DEL" => key_code = Some(Code::Delete),
            "ESCAPE" | "ESC" => key_code = Some(Code::Escape),
            "HOME" => key_code = Some(Code::Home),
            "END" => key_code = Some(Code::End),
            "PAGEUP" => key_code = Some(Code::PageUp),
            "PAGEDOWN" => key_code = Some(Code::PageDown),
            "UP" | "ARROWUP" => key_code = Some(Code::ArrowUp),
            "DOWN" | "ARROWDOWN" => key_code = Some(Code::ArrowDown),
            "LEFT" | "ARROWLEFT" => key_code = Some(Code::ArrowLeft),
            "RIGHT" | "ARROWRIGHT" => key_code = Some(Code::ArrowRight),
            "`" | "BACKQUOTE" => key_code = Some(Code::Backquote),
            _ => {}
        }
    }

    key_code.map(|code| {
        if modifiers.is_empty() {
            Shortcut::new(None, code)
        } else {
            Shortcut::new(Some(modifiers), code)
        }
    })
}

/// Initialize logging system
fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
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
}

/// Tauri command: Hide main window
#[tauri::command]
async fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
    keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
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
#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    tauri::process::restart(&app.env());
}

/// Toggle window visibility (like QuickClipboard's toggle_main_window_visibility)
fn toggle_window_visibility(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let current_state = keyboard_hook::get_window_state();
        
        if current_state == keyboard_hook::WindowState::Visible {
            // Hide window
            let _ = window.hide();
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Hidden);
            // Disable mouse monitoring when window is hidden
            input_monitor::disable_mouse_monitoring();
        } else {
            // Show window with always-on-top trick (like QuickClipboard)
            // NOTE: Do NOT call set_focus() - window is set to focusable=false
            let _ = window.show();
            let _ = window.set_always_on_top(false);
            std::thread::sleep(std::time::Duration::from_millis(10));
            let _ = window.set_always_on_top(true);
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
            // Enable mouse monitoring to detect clicks outside window
            input_monitor::enable_mouse_monitoring();
        }
    }
}

/// Tauri command: Enable Win+V replacement
/// This uses registry to disable system Win+V and Tauri's global_shortcut for our Win+V
#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // Unregister current custom shortcut
    let current_shortcut_str = {
        let guard = CURRENT_SHORTCUT.read().unwrap();
        guard.clone().unwrap_or_else(|| "Alt+C".to_string())
    };
    if let Some(shortcut) = parse_shortcut(&current_shortcut_str) {
        let _ = app.global_shortcut().unregister(shortcut);
    }
    
    // Disable system Win+V via registry (restart explorer to apply)
    win_v_registry::disable_win_v_hotkey(true)?;
    
    // Now register Win+V using Tauri's global_shortcut (system Win+V is disabled)
    let winv_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::KeyV);
    app.global_shortcut()
        .on_shortcut(winv_shortcut, |app, _shortcut, event| {
            // Only trigger on Pressed, not Released (like QuickClipboard)
            if event.state == ShortcutState::Pressed {
                toggle_window_visibility(app);
            }
        })
        .map_err(|e| format!("Failed to register Win+V shortcut: {}", e))?;
    
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
    let current_shortcut_str = {
        let guard = CURRENT_SHORTCUT.read().unwrap();
        guard.clone().unwrap_or_else(|| "Alt+C".to_string())
    };
    if let Some(shortcut) = parse_shortcut(&current_shortcut_str) {
        let _ = app.global_shortcut()
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

    // Get current shortcut
    let current_shortcut_str = {
        let guard = CURRENT_SHORTCUT.read().unwrap();
        guard.clone().unwrap_or_else(|| "Alt+C".to_string())
    };

    // Unregister current shortcut
    if let Some(current_sc) = parse_shortcut(&current_shortcut_str) {
        let _ = app.global_shortcut().unregister(current_sc);
    }

    // Register new shortcut
    app.global_shortcut()
        .register(new_sc)
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // Update global state
    {
        let mut guard = CURRENT_SHORTCUT.write().unwrap();
        *guard = Some(new_shortcut.clone());
    }

    Ok(new_shortcut)
}

/// Tauri command: Get current shortcut
#[tauri::command]
fn get_current_shortcut() -> String {
    let guard = CURRENT_SHORTCUT.read().unwrap();
    guard.clone().unwrap_or_else(|| "Alt+C".to_string())
}

/// Tauri command: Open settings window
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    // Check if settings window already exists
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    // Create new settings window (initially hidden to prevent white flash)
    let _window = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("/settings".into()),
    )
    .title("设置")
    .inner_size(800.0, 560.0)
    .min_inner_size(580.0, 480.0)
    .center()
    .decorations(false)
    .visible(false)
    .resizable(true)
    .build()
    .map_err(|e| format!("Failed to create settings window: {}", e))?;

    // Window will be shown by frontend after content is loaded
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // Load configuration and initialize database
            let config = AppConfig::load();
            let db_path = config.get_db_path();
            let db = Database::new(db_path).map_err(|e| e.to_string())?;

            // Initialize clipboard monitor
            let monitor = ClipboardMonitor::new();
            monitor.init(&db);

            // Create app state
            let state = Arc::new(AppState { db, monitor });

            // Load saved shortcut from settings
            let settings_repo = database::SettingsRepository::new(&state.db);
            let saved_shortcut = settings_repo.get("global_shortcut")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Alt+C".to_string());

            // Start clipboard monitoring
            state.monitor.start(app.handle().clone());
            app.manage(state);

            // Setup system tray
            let _ = tray::setup_tray(app.handle());
            
            // Initialize global shortcut state
            *CURRENT_SHORTCUT.write().unwrap() = Some(saved_shortcut.clone());
            
            // Register shortcut based on Win+V replacement setting
            let shortcut = if win_v_registry::is_win_v_hotkey_disabled() {
                Shortcut::new(Some(Modifiers::SUPER), Code::KeyV)
            } else {
                parse_shortcut(&saved_shortcut)
                    .unwrap_or_else(|| Shortcut::new(Some(Modifiers::ALT), Code::KeyC))
            };
            
            let _ = app.global_shortcut().on_shortcut(shortcut, |app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    toggle_window_visibility(app);
                }
            });

            // Set main window as non-focusable to prevent stealing focus from other apps
            // This allows hotkeys to work even when Start Menu or other system UI is open
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focusable(false);
                
                // Initialize and start input monitor for click-outside detection
                // This is necessary because non-focusable windows don't trigger onFocusChanged
                input_monitor::init(window);
                input_monitor::start_monitoring();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Window commands
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
            // Shortcut commands
            enable_winv_replacement,
            disable_winv_replacement,
            is_winv_replacement_enabled,
            update_shortcut,
            get_current_shortcut,
            // Clipboard commands
            commands::get_clipboard_items,
            commands::get_clipboard_item,
            commands::get_clipboard_count,
            commands::toggle_pin,
            commands::toggle_favorite,
            commands::delete_clipboard_item,
            commands::clear_history,
            commands::copy_to_clipboard,
            commands::paste_content,
            // Category commands
            commands::get_categories,
            commands::create_category,
            commands::delete_category,
            // Settings commands
            commands::get_setting,
            commands::set_setting,
            commands::get_all_settings,
            // Monitor commands
            commands::pause_monitor,
            commands::resume_monitor,
            commands::get_monitor_status,
            // Database commands
            commands::optimize_database,
            commands::vacuum_database,
            // Folder commands
            commands::select_folder_for_settings,
            commands::open_data_folder,
            // Autostart commands
            commands::is_autostart_enabled,
            commands::enable_autostart,
            commands::disable_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
