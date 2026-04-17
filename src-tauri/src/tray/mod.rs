use crate::commands::AppState;
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tracing::info;

/// 初始化系统托盘图标和菜单
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let icon_data = include_bytes!("../../icons/icon.png");
    let img = image::load_from_memory(icon_data)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = Image::new_owned(rgba.into_raw(), width, height);

    let pause_item = MenuItem::with_id(app, "toggle_pause", "暂停监控", true, None::<&str>)?;
    let shortcut_item = MenuItem::with_id(app, "toggle_shortcuts", "禁用快捷键", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", "重启程序", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&pause_item, &shortcut_item, &separator1, &settings_item, &restart_item, &separator2, &quit_item],
    )?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("ElegantClipboard")
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "toggle_pause" => {
                    if let Some(state) = app.try_state::<Arc<AppState>>() {
                        let paused = state.monitor.toggle_user_pause();
                        let _ = pause_item.set_text(if paused { "恢复监控" } else { "暂停监控" });
                        if let Some(tray) = app.tray_by_id("main-tray") {
                            let tip = if paused { "ElegantClipboard (已暂停)" } else { "ElegantClipboard" };
                            let _ = tray.set_tooltip(Some(tip));
                        }
                    }
                }
                "toggle_shortcuts" => {
                    let disabled = crate::toggle_shortcuts_disabled(app);
                    let _ = shortcut_item.set_text(if disabled { "恢复快捷键" } else { "禁用快捷键" });
                }
                _ => handle_menu_event(app, id),
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let is_visible = window.is_visible().unwrap_or(false);
                    let is_minimized = window.is_minimized().unwrap_or(false);
                    if is_visible && !is_minimized {
                        crate::commands::window::hide_main_window(tray.app_handle(), &window);
                    } else if !crate::keyboard_hook::was_recently_hidden(300) {
                        crate::commands::window::show_main_window(tray.app_handle(), &window);
                    }
                }
            }
        })
        .build(app)?;

    info!("System tray initialized");
    Ok(())
}

/// 处理托盘菜单事件
fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "settings" => {
            let _ = open_settings_window(app);
        }
        "restart" => {
            // 使用支持 UAC 提权的重启逻辑
            // app.restart() 不触发提权，用自定义重启
            crate::commands::window::save_main_window_placement(app);
            if crate::admin_launch::restart_app() {
                app.exit(0);
            } else {
                app.restart();
            }
        }
        "quit" => {
            crate::commands::window::save_main_window_placement(app);
            app.exit(0);
        }
        _ => {}
    }
}

/// 打开或聚焦设置窗口，居中于主窗口所在的显示器
pub(crate) fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    // 设置窗口已存在则聚焦
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        "settings",
        tauri::WebviewUrl::App("/settings".into()),
    )
    .title("设置")
    .inner_size(800.0, 560.0)
    .min_inner_size(580.0, 480.0)
    .decorations(false)
    .transparent(true)
    .shadow(true)
    .visible(false)
    .resizable(true);

    // 居中于主窗口所在显示器（使用物理像素避免 DPI 换算误差）
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

    let window = builder
        .build()
        .map_err(|e| format!("创建设置窗口失败: {}", e))?;

    // 构建后设置物理位置，绕过逻辑→物理坐标换算歧义
    if let Some(pos) = phys_pos {
        let _ = window.set_position(tauri::Position::Physical(pos));
    }

    Ok(())
}
