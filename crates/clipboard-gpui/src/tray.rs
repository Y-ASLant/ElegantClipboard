use anyhow::{Context, Result};
use clipboard_core::preferences::LanguagePreference;
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::{
        Input::KeyboardAndMouse::GetDoubleClickTime,
        WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, IsIconic, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
        },
    },
};

#[derive(Clone, Copy)]
pub enum TrayCommand {
    Show,
    Toggle,
    Settings,
    TogglePause,
    Quit,
}

pub fn create(
    sender: async_channel::Sender<TrayCommand>,
    language: LanguagePreference,
) -> Result<TrayIcon> {
    let menu = Menu::new();
    let english = language == LanguagePreference::English;
    let show = MenuItem::with_id("show", if english { "Open" } else { "打开" }, true, None);
    let settings = MenuItem::with_id(
        "settings",
        if english { "Settings" } else { "设置" },
        true,
        None,
    );
    let pause = MenuItem::with_id(
        "toggle-pause",
        if english {
            "Pause / Resume Recording"
        } else {
            "暂停 / 恢复记录"
        },
        true,
        None,
    );
    let quit = MenuItem::with_id("quit", if english { "Quit" } else { "退出" }, true, None);
    menu.append(&show).context("无法创建托盘菜单")?;
    menu.append(&settings).context("无法创建托盘菜单")?;
    menu.append(&pause).context("无法创建托盘菜单")?;
    menu.append(&quit).context("无法创建托盘菜单")?;
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
            window.activate_window();
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
