use anyhow::{Context, Result};
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowPos, ShowWindow,
    },
};

#[derive(Clone, Copy)]
pub enum TrayCommand {
    Show,
    Quit,
}

pub fn create(sender: async_channel::Sender<TrayCommand>) -> Result<TrayIcon> {
    let menu = Menu::new();
    let show = MenuItem::with_id("show", "打开剪贴板历史", true, None);
    let quit = MenuItem::with_id("quit", "退出 ElegantClipboard", true, None);
    menu.append(&show).context("无法创建托盘菜单")?;
    menu.append(&quit).context("无法创建托盘菜单")?;
    let icon = Icon::from_rgba(icon_pixels(), 32, 32).context("无法创建托盘图标")?;
    let tray = TrayIconBuilder::new()
        .with_tooltip("ElegantClipboard · 剪贴板历史")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .context("无法启动系统托盘")?;
    let menu_sender = sender.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = match event.id.as_ref() {
            "show" => Some(TrayCommand::Show),
            "quit" => Some(TrayCommand::Quit),
            _ => None,
        };
        if let Some(command) = command {
            let _ = menu_sender.try_send(command);
        }
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } | TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            let _ = sender.try_send(TrayCommand::Show);
        }
    }));
    Ok(tray)
}

pub fn set_window_visible(window: &Window, visible: bool) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        unsafe {
            let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
        if visible {
            window.activate_window();
        }
    }
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
