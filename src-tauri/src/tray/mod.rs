use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tracing::info;

/// Setup system tray icon and menu
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    // Load tray icon from raw RGBA data
    let icon_data = include_bytes!("../../icons/icon.png");
    let img = image::load_from_memory(icon_data)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = Image::new_owned(rgba.into_raw(), width, height);

    // Create menu items
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", "重启程序", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;

    // Build menu
    let menu = Menu::with_items(
        app,
        &[&settings_item, &restart_item, &separator, &quit_item],
    )?;

    // Create tray icon
    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("ElegantClipboard")
        .on_menu_event(move |app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                // Left click to show/hide window
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        // Disable mouse monitoring when hiding
                        crate::input_monitor::disable_mouse_monitoring();
                        crate::keyboard_hook::set_window_state(
                            crate::keyboard_hook::WindowState::Hidden,
                        );
                        // Emit event to frontend so it can reset state while hidden
                        let _ = window.emit("window-hidden", ());
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                        // Enable mouse monitoring when showing
                        crate::input_monitor::enable_mouse_monitoring();
                        crate::keyboard_hook::set_window_state(
                            crate::keyboard_hook::WindowState::Visible,
                        );
                        // Emit event to frontend for cache invalidation
                        let _ = window.emit("window-shown", ());
                    }
                }
            }
        })
        .build(app)?;

    info!("System tray initialized");
    Ok(())
}

/// Handle tray menu events
fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "settings" => {
            info!("Opening settings from tray");
            open_settings_window_sync(app);
        }
        "restart" => {
            info!("Restarting application from tray");
            app.restart();
        }
        "quit" => {
            info!("Quitting application from tray");
            app.exit(0);
        }
        _ => {}
    }
}

/// Open settings window (sync version for tray menu)
fn open_settings_window_sync<R: Runtime>(app: &AppHandle<R>) {
    // Check if settings window already exists
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    // Create new settings window
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
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

    if let Ok(window) = builder.build() {
        if let Some(pos) = phys_pos {
            let _ = window.set_position(tauri::Position::Physical(pos));
        }
    }
}

/// Update tray tooltip with item count
#[allow(dead_code)]
pub fn update_tray_tooltip<R: Runtime>(app: &AppHandle<R>, count: i64) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let tooltip = format!("ElegantClipboard - {} 条记录", count);
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}
