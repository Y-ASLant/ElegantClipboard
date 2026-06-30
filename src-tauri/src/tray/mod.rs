pub mod tray_i18n;

use crate::commands::AppState;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tracing::info;
use tray_i18n::TrayI18n;

const MAIN_TRAY_ID: &str = "main-tray";

/// 运行时托盘菜单项引用，用于动态更新文本
struct TrayMenuItems {
    pause_item: MenuItem<tauri::Wry>,
    shortcut_item: MenuItem<tauri::Wry>,
    settings_item: MenuItem<tauri::Wry>,
    restart_item: MenuItem<tauri::Wry>,
    quit_item: MenuItem<tauri::Wry>,
}

/// 全局托盘菜单项 + 当前语言，用于语言切换时更新菜单文本
static TRAY_STATE: Mutex<Option<(TrayMenuItems, String)>> = Mutex::new(None);

/// 从数据库读取语言设置（缺失时回退 zh-CN）
fn read_locale(app: &AppHandle) -> String {
    app.try_state::<Arc<AppState>>()
        .map(|state| {
            crate::database::SettingsRepository::new(&state.db)
                .get("language")
                .ok()
                .flatten()
                .unwrap_or_else(|| "zh-CN".into())
        })
        .unwrap_or_else(|| "zh-CN".into())
}

/// 初始化系统托盘图标和菜单
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let icon_data = include_bytes!("../../icons/icon.png");
    let img = image::load_from_memory(icon_data)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let icon = Image::new_owned(rgba.into_raw(), width, height);

    let locale = read_locale(app);
    let i18n = TrayI18n::from_locale(&locale);

    let pause_item =
        MenuItem::with_id(app, "toggle_pause", &i18n.pause_monitor, true, None::<&str>)?;
    let shortcut_item = MenuItem::with_id(
        app,
        "toggle_shortcuts",
        &i18n.disable_shortcuts,
        true,
        None::<&str>,
    )?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings", &i18n.settings, true, None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", &i18n.restart, true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", &i18n.quit, true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &pause_item,
            &shortcut_item,
            &separator1,
            &settings_item,
            &restart_item,
            &separator2,
            &quit_item,
        ],
    )?;

    // 保存菜单项引用，供语言切换时更新
    *TRAY_STATE.lock() = Some((
        TrayMenuItems {
            pause_item: pause_item.clone(),
            shortcut_item: shortcut_item.clone(),
            settings_item: settings_item.clone(),
            restart_item: restart_item.clone(),
            quit_item: quit_item.clone(),
        },
        locale.clone(),
    ));

    let tray = TrayIconBuilder::with_id(MAIN_TRAY_ID)
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
                        // 从全局状态读取当前语言的翻译
                        let tip;
                        {
                            let guard = TRAY_STATE.lock();
                            let i18n = guard
                                .as_ref()
                                .map(|(_, loc)| TrayI18n::from_locale(loc))
                                .unwrap_or_else(TrayI18n::zh_cn);
                            let _ = pause_item.set_text(if paused {
                                &i18n.resume_monitor
                            } else {
                                &i18n.pause_monitor
                            });
                            tip = if paused {
                                i18n.paused_tip
                            } else {
                                "ElegantClipboard".into()
                            };
                        }
                        if let Some(tray) = app.tray_by_id(MAIN_TRAY_ID) {
                            let _ = tray.set_tooltip(Some(&tip));
                        }
                    }
                }
                "toggle_shortcuts" => {
                    let disabled = crate::toggle_shortcuts_disabled(app);
                    let guard = TRAY_STATE.lock();
                    let i18n = guard
                        .as_ref()
                        .map(|(_, loc)| TrayI18n::from_locale(loc))
                        .unwrap_or_else(TrayI18n::zh_cn);
                    let _ = shortcut_item.set_text(if disabled {
                        &i18n.restore_shortcuts
                    } else {
                        &i18n.disable_shortcuts
                    });
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
                && let Some(window) = tray.app_handle().get_webview_window("main")
            {
                let is_visible = window.is_visible().unwrap_or(false);
                let is_minimized = window.is_minimized().unwrap_or(false);
                if is_visible && !is_minimized {
                    crate::commands::window::hide_main_window(tray.app_handle(), &window);
                } else if !crate::keyboard_hook::was_recently_hidden(300) {
                    crate::commands::window::show_main_window(tray.app_handle(), &window);
                }
            }
        })
        .build(app)?;

    let _ = tray.set_visible(load_tray_visibility(app));

    info!("System tray initialized (locale: {locale})");
    Ok(())
}

fn load_tray_visibility(app: &AppHandle) -> bool {
    app.try_state::<Arc<AppState>>().is_none_or(|state| {
        let repo = crate::database::SettingsRepository::new(&state.db);
        repo.get("tray_icon_visible")
            .ok()
            .flatten()
            .is_none_or(|value| value != "false")
    })
}

pub(crate) fn set_tray_visibility(app: &AppHandle, visible: bool) -> Result<(), String> {
    let tray = app
        .tray_by_id(MAIN_TRAY_ID)
        .ok_or_else(|| "Tray icon not initialized".to_string())?;
    tray.set_visible(visible).map_err(|e| e.to_string())
}

/// 更新托盘菜单语言（供前端语言切换时调用）
pub(crate) fn update_tray_language(_app: &AppHandle, locale: &str) {
    let mut guard = TRAY_STATE.lock();
    let Some((items, current)) = guard.as_mut() else {
        return;
    };
    if current == locale {
        return;
    }
    *current = locale.to_string();

    let i18n = TrayI18n::from_locale(locale);
    let _ = items.pause_item.set_text(&i18n.pause_monitor);
    let _ = items.shortcut_item.set_text(&i18n.disable_shortcuts);
    let _ = items.settings_item.set_text(&i18n.settings);
    let _ = items.restart_item.set_text(&i18n.restart);
    let _ = items.quit_item.set_text(&i18n.quit);
    info!("Tray menu language updated to: {locale}");
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
    let app = app.clone();
    crate::main_thread::run_on_ui_thread(&app.clone(), move || open_settings_window_inner(&app))
        .and_then(|r| r)
}

fn open_settings_window_inner(app: &AppHandle) -> Result<(), String> {
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
        .map_err(|e| format!("创建设置窗口失败: {e}"))?;

    // 构建后设置物理位置，绕过逻辑→物理坐标换算歧义
    if let Some(pos) = phys_pos {
        let _ = window.set_position(tauri::Position::Physical(pos));
    }

    Ok(())
}
