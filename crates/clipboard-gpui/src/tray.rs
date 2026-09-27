use anyhow::{Context, Result};
use clipboard_core::preferences::{LanguagePreference, ToolbarButton, ToolbarPreference};
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
};
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetDoubleClickTime, VK_MENU},
        WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowPos, ShowWindow,
        },
    },
};

#[derive(Clone, Copy)]
pub enum TrayCommand {
    Show,
    Toggle,
    Settings,
    ClearHistory,
    TogglePin,
    TogglePause,
    Quit,
}

pub fn create(
    sender: async_channel::Sender<TrayCommand>,
    language: LanguagePreference,
    toolbar: ToolbarPreference,
    pinned: bool,
) -> Result<TrayIcon> {
    let menu = create_menu(language, toolbar, pinned)?;
    let icon = Icon::from_rgba(icon_pixels(), 32, 32).context("无法创建托盘图标")?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("ElegantClipboard")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .context("无法启动系统托盘")?;
    let menu_sender = sender.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = match event.id.as_ref() {
            "show" => Some(TrayCommand::Show),
            "clear-history" => Some(TrayCommand::ClearHistory),
            "toggle-pin" => Some(TrayCommand::TogglePin),
            "settings" => Some(TrayCommand::Settings),
            "toggle-pause" => Some(TrayCommand::TogglePause),
            "quit" => Some(TrayCommand::Quit),
            _ => None,
        };
        if let Some(command) = command {
            let _ = menu_sender.try_send(command);
        }
    }));
    let last_left_click = Mutex::new(None::<Instant>);
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
            && let Ok(mut previous) = last_left_click.lock()
        {
            let now = Instant::now();
            let double_click_interval =
                Duration::from_millis(unsafe { GetDoubleClickTime() } as u64);
            if previous.is_none_or(|last| now.duration_since(last) > double_click_interval) {
                let _ = sender.try_send(TrayCommand::Toggle);
            }
            *previous = Some(now);
        }
    }));
    Ok(tray)
}

pub fn update_menu(
    tray: &TrayIcon,
    language: LanguagePreference,
    toolbar: ToolbarPreference,
    pinned: bool,
) -> Result<()> {
    tray.set_menu(Some(Box::new(create_menu(language, toolbar, pinned)?)));
    Ok(())
}

fn create_menu(
    language: LanguagePreference,
    toolbar: ToolbarPreference,
    pinned: bool,
) -> Result<Menu> {
    let menu = Menu::new();
    let english = language == LanguagePreference::English;
    menu.append(&MenuItem::with_id(
        "show",
        if english { "Open" } else { "打开" },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    for item in toolbar.items {
        if !item.visible && item.button != ToolbarButton::Settings {
            continue;
        }
        match item.button {
            ToolbarButton::Clear => menu.append(&MenuItem::with_id(
                "clear-history",
                if english {
                    "Clear history"
                } else {
                    "清理历史"
                },
                true,
                None,
            )),
            ToolbarButton::Pin => menu.append(&CheckMenuItem::with_id(
                "toggle-pin",
                if english {
                    "Pin window"
                } else {
                    "置顶窗口"
                },
                true,
                pinned,
                None,
            )),
            ToolbarButton::Settings => menu.append(&MenuItem::with_id(
                "settings",
                if english { "Settings" } else { "设置" },
                true,
                None,
            )),
            ToolbarButton::Batch => continue,
        }
        .context("无法创建托盘菜单")?;
    }
    menu.append(&MenuItem::with_id(
        "toggle-pause",
        if english {
            "Pause / Resume Recording"
        } else {
            "暂停 / 恢复记录"
        },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    menu.append(&MenuItem::with_id(
        "quit",
        if english { "Quit" } else { "退出" },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    Ok(menu)
}

pub fn set_window_visible(window: &Window, visible: bool) {
    if let Some(hwnd) = window_hwnd(window) {
        unsafe {
            let command = if !visible {
                SW_HIDE
            } else if IsIconic(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(hwnd, command);
        }
        if visible {
            if unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0 {
                // GPUI activation injects Alt down/up, which releases a held Alt+C
                // chord and prevents pressing C again until Alt is physically reset.
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                }
            } else {
                window.activate_window();
            }
        }
    }
}

pub fn window_hwnd(window: &Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}

pub fn is_window_shown(window: &Window) -> bool {
    window_hwnd(window).is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) }.as_bool())
}

pub fn is_window_minimized(window: &Window) -> bool {
    window_hwnd(window).is_some_and(|hwnd| unsafe { IsIconic(hwnd) }.as_bool())
}

pub fn is_tray_click(tray: &TrayIcon, position: POINT) -> bool {
    tray.rect().is_some_and(|rect| {
        let x = f64::from(position.x);
        let y = f64::from(position.y);
        x >= rect.position.x
            && x < rect.position.x + f64::from(rect.size.width)
            && y >= rect.position.y
            && y < rect.position.y + f64::from(rect.size.height)
    })
}

pub fn set_window_topmost(window: &Window, topmost: bool) -> Result<()> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| anyhow::anyhow!("无法读取窗口句柄：{error}"))?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        anyhow::bail!("当前窗口不是 Win32 窗口");
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(if topmost {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            }),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
        .context("无法更新窗口置顶状态")?;
    }
    Ok(())
}

fn icon_pixels() -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let index = (y * 32 + x) * 4;
            let color = if (5..27).contains(&x) && (4..29).contains(&y) {
                if (10..23).contains(&x) && (10..13).contains(&y)
                    || (10..23).contains(&x) && (16..19).contains(&y)
                    || (10..19).contains(&x) && (22..25).contains(&y)
                {
                    [255, 255, 255, 255]
                } else {
                    [55, 96, 191, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
            pixels[index..index + 4].copy_from_slice(&color);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use tray_icon::menu::MenuItemKind;

    fn visible_labels(menu: &Menu) -> Vec<String> {
        menu.items()
            .iter()
            .map(|item| match item {
                MenuItemKind::MenuItem(item) => item.text(),
                MenuItemKind::Check(item) => item.text(),
                _ => panic!("unexpected tray menu item"),
            })
            .collect()
    }

    #[test]
    fn tray_actions_follow_visibility_order_language_and_pin_state() -> Result<()> {
        let toolbar = ToolbarPreference::default();
        let chinese = create_menu(LanguagePreference::Chinese, toolbar, false)?;
        assert_eq!(
            visible_labels(&chinese),
            [
                "打开",
                "清理历史",
                "置顶窗口",
                "设置",
                "暂停 / 恢复记录",
                "退出"
            ]
        );
        assert!(
            !chinese.items()[2]
                .as_check_menuitem()
                .expect("pin is a checked menu item")
                .is_checked()
        );

        let toolbar = toolbar
            .move_before_or_after(ToolbarButton::Pin, ToolbarButton::Clear, false)
            .expect("pin can move before clear")
            .with_visibility(ToolbarButton::Clear, false)
            .expect("clear can be hidden");
        let english = create_menu(LanguagePreference::English, toolbar, true)?;
        assert_eq!(
            visible_labels(&english),
            [
                "Open",
                "Pin window",
                "Settings",
                "Pause / Resume Recording",
                "Quit"
            ]
        );
        assert!(
            english.items()[1]
                .as_check_menuitem()
                .expect("pin is a checked menu item")
                .is_checked()
        );
        Ok(())
    }
}
