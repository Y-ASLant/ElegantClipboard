use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tracing::info;

/// 初始化系统托盘图标和菜单
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let icon_data = include_bytes!("../../icons/icon.png");
    let img = image::load_from_memory(icon_data)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = Image::new_owned(rgba.into_raw(), width, height);

    let settings_item = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", "重启程序", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&settings_item, &restart_item, &separator, &quit_item],
    )?;

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
                // 左键点击：切换窗口可见性
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        crate::save_window_size_if_enabled(tray.app_handle(), &window);
                        let _ = window.hide();
                        crate::input_monitor::disable_mouse_monitoring();
                        crate::keyboard_hook::set_window_state(
                            crate::keyboard_hook::WindowState::Hidden,
                        );
                        crate::commands::hide_image_preview_window(tray.app_handle());
                        let _ = window.emit("window-hidden", ());
                    } else if !crate::keyboard_hook::was_recently_hidden(300) {
                        // 若窗口刚被 handle_click_outside 隐藏（<300ms），
                        // 说明本次托盘点击的意图是隐藏，不应再显示
                        let _ = window.show();
                        crate::input_monitor::enable_mouse_monitoring();
                        crate::keyboard_hook::set_window_state(
                            crate::keyboard_hook::WindowState::Visible,
                        );
                        let _ = window.emit("window-shown", ());
                    }
                }
            }
        })
        .build(app)?;

    info!("系统托盘已初始化");
    Ok(())
}

/// 处理托盘菜单事件
fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match id {
        "settings" => {
            let _ = open_settings_window(app);
        }
        "restart" => {
            // 使用支持 UAC 提权的重启逻辑
            // （直接 app.restart() 不会触发管理员提权）
            if crate::admin_launch::restart_app() {
                app.exit(0);
            } else {
                app.restart();
            }
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

/// 打开或聚焦设置窗口，居中于主窗口所在的显示器
pub(crate) fn open_settings_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
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

/// 更新托盘提示文本
#[allow(dead_code)]
pub fn update_tray_tooltip<R: Runtime>(app: &AppHandle<R>, count: i64) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let tooltip = format!("ElegantClipboard - {} 条记录", count);
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}
