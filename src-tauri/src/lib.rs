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
use shortcut::parse_shortcut;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

/// Global state for current shortcut (parking_lot::RwLock: no poison, consistent with codebase)
static CURRENT_SHORTCUT: parking_lot::RwLock<Option<String>> = parking_lot::RwLock::new(None);

/// Initialize logging system
fn init_logging() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .finish();

    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("Failed to set tracing subscriber: {err}");
    }
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
            // Hide image preview window (onMouseLeave won't fire when main window disappears)
            commands::hide_image_preview_window(app);
            // Emit event to frontend so it can reset state while hidden
            let _ = window.emit("window-hidden", ());
        } else {
            // Check if follow_cursor is enabled
            let follow_cursor = app
                .try_state::<std::sync::Arc<commands::AppState>>()
                .map(|state| {
                    let settings_repo = database::SettingsRepository::new(&state.db);
                    settings_repo
                        .get("follow_cursor")
                        .ok()
                        .flatten()
                        .map(|v| v != "false")
                        .unwrap_or(true) // Default to true
                })
                .unwrap_or(true);

            // Position window at cursor before showing (if enabled)
            if follow_cursor {
                if let Err(e) = positioning::position_at_cursor(&window) {
                    tracing::warn!("Failed to position window at cursor: {}", e);
                }
            }

            // Show window with always-on-top trick (like QuickClipboard)
            // NOTE: Do NOT call set_focus() - window is set to focusable=false
            let _ = window.show();
            let _ = window.set_always_on_top(false);
            std::thread::sleep(std::time::Duration::from_millis(10));
            let _ = window.set_always_on_top(true);
            keyboard_hook::set_window_state(keyboard_hook::WindowState::Visible);
            // Enable mouse monitoring to detect clicks outside window
            input_monitor::enable_mouse_monitoring();
            // Emit event to frontend for cache invalidation
            let _ = window.emit("window-shown", ());
        }
    }
}

/// Tauri command: Enable Win+V replacement
/// This uses registry to disable system Win+V and Tauri's global_shortcut for our Win+V
#[tauri::command]
async fn enable_winv_replacement(app: tauri::AppHandle) -> Result<(), String> {
    // Unregister current custom shortcut
    if let Some(shortcut) = parse_shortcut(&get_current_shortcut()) {
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
        .map_err(|e| format!("Failed to create preview window: {}", e))?
    };

    // Always set position/size in physical pixels to avoid mixed-DPI conversion errors
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width: win_width as u32,
        height: win_height as u32,
    }));
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
        x: win_x as i32,
        y: win_y as i32,
    }));

    if newly_created {
        // First creation: wait for HTML to load before emitting events
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    // Ensure always_on_top is active (in case main window focus state affected window hierarchy)
    let _ = window.set_always_on_top(true);

    // Send image path + initial CSS size to the preview window
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

/// Tauri command: Hide image preview window and clear its content
#[tauri::command]
async fn hide_image_preview(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("image-preview") {
        let _ = window.hide();
        // Clear the image so next show doesn't flash the old content
        let _ = window.emit("image-preview-clear", ());
    }
}

/// Tauri command: Open text editor window
#[tauri::command]
async fn open_text_editor_window(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let label = format!("text-editor-{}", id);

    // If editor for this item already exists, focus it
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
    .map_err(|e| format!("Failed to create editor window: {}", e))?;

    // Window will be shown by frontend after content is loaded
    let _ = window;
    Ok(())
}

/// Tauri command: Open settings window
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    // Check if settings window already exists
    if let Some(window) = app.get_webview_window("settings") {
        // Unminimize if the window is minimized
        let _ = window.unminimize();
        // Show the window (in case it's hidden)
        let _ = window.show();
        // Set focus to bring it to front
        let _ = window.set_focus();
        return Ok(());
    }

    // Calculate center position on the same monitor as the main window
    let mut builder = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("/settings".into()),
    )
    .title("设置")
    .inner_size(800.0, 560.0)
    .min_inner_size(580.0, 480.0)
    .decorations(false)
    .visible(false)
    .resizable(true);

    // Center on the monitor where the main window is, not the primary monitor.
    // Use physical pixel coordinates to avoid mixed-DPI conversion errors.
    let mut phys_pos: Option<tauri::PhysicalPosition<i32>> = None;
    if let Some(main_win) = app.get_webview_window("main") {
        if let (Ok(pos), Ok(size)) = (main_win.outer_position(), main_win.outer_size()) {
            let center_x = pos.x + size.width as i32 / 2;
            let center_y = pos.y + size.height as i32 / 2;
            if let Ok(Some(monitor)) = main_win.available_monitors().map(|monitors| {
                monitors.into_iter().find(|m| {
                    let mp = m.position();
                    let ms = m.size();
                    center_x >= mp.x
                        && center_x < mp.x + ms.width as i32
                        && center_y >= mp.y
                        && center_y < mp.y + ms.height as i32
                })
            }) {
                let mp = monitor.position();
                let ms = monitor.size();
                let scale = monitor.scale_factor();
                // Calculate window physical size and center within monitor physical bounds
                let win_phys_w = (800.0 * scale) as i32;
                let win_phys_h = (560.0 * scale) as i32;
                let x = mp.x + (ms.width as i32 - win_phys_w) / 2;
                let y = mp.y + (ms.height as i32 - win_phys_h) / 2;
                phys_pos = Some(tauri::PhysicalPosition::new(x, y));
            } else {
                builder = builder.center();
            }
        } else {
            builder = builder.center();
        }
    } else {
        builder = builder.center();
    }

    let window = builder
        .build()
        .map_err(|e| format!("Failed to create settings window: {}", e))?;

    // Apply physical position after build to bypass logical-to-physical conversion ambiguity
    if let Some(pos) = phys_pos {
        let _ = window.set_position(tauri::Position::Physical(pos));
    }

    // Window will be shown by frontend after content is loaded
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

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
            // Load configuration and initialize database
            let config = AppConfig::load();
            let db_path = config.get_db_path();
            let images_path = config.get_images_path();
            let db = Database::new(db_path).map_err(|e| e.to_string())?;

            // Initialize clipboard monitor with configured images path
            let monitor = ClipboardMonitor::new();
            monitor.init(&db, images_path);

            // Create app state
            let state = Arc::new(AppState { db, monitor });

            // Load saved shortcut from settings
            let settings_repo = database::SettingsRepository::new(&state.db);
            let saved_shortcut = settings_repo
                .get("global_shortcut")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Alt+C".to_string());

            // Start clipboard monitoring
            state.monitor.start(app.handle().clone());
            app.manage(state);

            // Setup system tray
            let _ = tray::setup_tray(app.handle());

            // Initialize global shortcut state
            *CURRENT_SHORTCUT.write() = Some(saved_shortcut.clone());

            // Register shortcut based on Win+V replacement setting
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

            // Set main window as non-focusable to prevent stealing focus from other apps
            // This allows hotkeys to work even when Start Menu or other system UI is open
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focusable(false);

                // Initialize and start input monitor for click-outside detection
                // This is necessary because non-focusable windows don't trigger onFocusChanged
                input_monitor::init(window);
                input_monitor::start_monitoring();
            }

            // 自启动机制迁移：管理员模式用任务计划程序，普通模式用注册表 Run
            {
                use tauri_plugin_autostart::ManagerExt;
                let is_admin =
                    admin_launch::is_admin_launch_enabled() && admin_launch::is_running_as_admin();

                if is_admin && app.autolaunch().is_enabled().unwrap_or(false) {
                    if task_scheduler::create_autostart_task().is_ok() {
                        let _ = app.autolaunch().disable();
                        tracing::info!("自启动迁移: 注册表 Run → 任务计划程序");
                    }
                } else if !admin_launch::is_admin_launch_enabled()
                    && task_scheduler::is_autostart_task_exists()
                {
                    if app.autolaunch().enable().is_ok() {
                        let _ = task_scheduler::delete_autostart_task();
                        tracing::info!("自启动迁移: 任务计划程序 → 注册表 Run");
                    }
                }
            }

            // Start system accent color watcher for live theme updates
            #[cfg(target_os = "windows")]
            commands::settings::start_accent_color_watcher(app.handle().clone());

            // Send startup notification so user knows the app is running in tray
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
            show_image_preview,
            hide_image_preview,
            open_text_editor_window,
            set_window_pinned,
            is_window_pinned,
            // Admin launch commands
            is_admin_launch_enabled,
            enable_admin_launch,
            disable_admin_launch,
            is_running_as_admin,
            // Shortcut commands
            enable_winv_replacement,
            disable_winv_replacement,
            is_winv_replacement_enabled,
            update_shortcut,
            get_current_shortcut,
            // Update commands
            check_for_update,
            download_update,
            install_update,
            // Clipboard commands
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
            commands::clipboard::update_text_content,
            // Settings, monitor, database, folder, autostart commands
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
            // File operation commands
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
