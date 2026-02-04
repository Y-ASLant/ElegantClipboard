use crate::commands::AppState;
use std::sync::Arc;
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
    // We'll use a simple colored square as placeholder
    // In production, you should use proper icon loading
    let icon_data = include_bytes!("../../icons/32x32.png");
    let img = image::load_from_memory(icon_data)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = Image::new_owned(rgba.into_raw(), width, height);

    // Create menu items
    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, "pause", "暂停监听", true, None::<&str>)?;
    let resume_item = MenuItem::with_id(app, "resume", "恢复监听", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    // Build menu
    let menu = Menu::with_items(
        app,
        &[
            &show_item,
            &separator,
            &pause_item,
            &resume_item,
            &separator2,
            &quit_item,
        ],
    )?;

    // Create tray icon
    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .menu(&menu)
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
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
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
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "pause" => {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                state.monitor.pause();
                let _ = app.emit("monitor-paused", ());
                info!("Clipboard monitoring paused from tray");
            }
        }
        "resume" => {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                state.monitor.resume();
                let _ = app.emit("monitor-resumed", ());
                info!("Clipboard monitoring resumed from tray");
            }
        }
        "quit" => {
            info!("Quitting application from tray");
            app.exit(0);
        }
        _ => {}
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
