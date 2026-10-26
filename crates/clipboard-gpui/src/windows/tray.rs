use anyhow::{Context, Result};
use clipboard_core::preferences::LanguagePreference;
use std::{sync::Mutex, time::Instant};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

#[derive(Clone, Copy)]
pub enum TrayCommand {
    Toggle,
    Settings,
    ClearHistory,
    TogglePause,
    Quit,
}

pub fn create(
    sender: async_channel::Sender<TrayCommand>,
    language: LanguagePreference,
    paused: bool,
    monitoring: bool,
) -> Result<TrayIcon> {
    let menu = create_menu(language, paused, monitoring)?;
    let icon = app_icon()?;
    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip_text(language, paused, monitoring))
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .context("无法启动系统托盘")?;
    let menu_sender = sender.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let command = match event.id.as_ref() {
            "clear-history" => Some(TrayCommand::ClearHistory),
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
            let double_click_interval = super::input::double_click_interval();
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
    paused: bool,
    monitoring: bool,
) -> Result<()> {
    tray.set_menu(Some(Box::new(create_menu(language, paused, monitoring)?)));
    Ok(())
}

/// 托盘悬浮提示随监听状态变化，避免“图标在却没在记录”的误判。
fn tooltip_text(language: LanguagePreference, paused: bool, monitoring: bool) -> String {
    if !monitoring {
        if language == LanguagePreference::English {
            "ElegantClipboard (clipboard monitoring off)".into()
        } else {
            "ElegantClipboard（未监听剪贴板）".into()
        }
    } else if paused {
        if language == LanguagePreference::English {
            "ElegantClipboard (recording paused)".into()
        } else {
            "ElegantClipboard（已暂停记录）".into()
        }
    } else {
        "ElegantClipboard".into()
    }
}

/// 暂停菜单项按状态显示将要执行的动作；未监听时禁用并如实标注。
fn pause_item_label(
    language: LanguagePreference,
    paused: bool,
    monitoring: bool,
) -> (&'static str, bool) {
    let english = language == LanguagePreference::English;
    if !monitoring {
        (
            if english {
                "Monitoring disabled"
            } else {
                "监听未启用"
            },
            false,
        )
    } else if paused {
        (
            if english {
                "Resume recording"
            } else {
                "恢复记录"
            },
            true,
        )
    } else {
        (
            if english {
                "Pause recording"
            } else {
                "暂停记录"
            },
            true,
        )
    }
}

fn create_menu(language: LanguagePreference, paused: bool, monitoring: bool) -> Result<Menu> {
    let menu = Menu::new();
    let english = language == LanguagePreference::English;
    let (pause_label, pause_enabled) = pause_item_label(language, paused, monitoring);
    menu.append(&MenuItem::with_id(
        "toggle-pause",
        pause_label,
        pause_enabled,
        None,
    ))
    .context("无法创建托盘菜单")?;
    menu.append(&MenuItem::with_id(
        "clear-history",
        if english {
            "Clear history"
        } else {
            "清理历史"
        },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    menu.append(&MenuItem::with_id(
        "settings",
        if english { "Settings" } else { "设置" },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    menu.append(&MenuItem::with_id(
        "quit",
        if english {
            "Exit application"
        } else {
            "退出程序"
        },
        true,
        None,
    ))
    .context("无法创建托盘菜单")?;
    Ok(menu)
}

pub fn is_tray_click(tray: &TrayIcon, position: (i32, i32)) -> bool {
    tray.rect().is_some_and(|rect| {
        let x = f64::from(position.0);
        let y = f64::from(position.1);
        x >= rect.position.x
            && x < rect.position.x + f64::from(rect.size.width)
            && y >= rect.position.y
            && y < rect.position.y + f64::from(rect.size.height)
    })
}

fn app_icon() -> Result<Icon> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../../../../App.ico"),
        image::ImageFormat::Ico,
    )
    .context("无法读取应用图标")?;
    let pixels = image::imageops::resize(
        &image.to_rgba8(),
        32,
        32,
        image::imageops::FilterType::Lanczos3,
    );
    Icon::from_rgba(pixels.into_raw(), 32, 32).context("无法创建托盘图标")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_recording_control_tracks_monitoring_availability() -> Result<()> {
        for language in [LanguagePreference::Chinese, LanguagePreference::English] {
            for monitoring in [false, true] {
                for paused in [false, true] {
                    let menu = create_menu(language, paused, monitoring)?;
                    let items = menu.items();
                    let pause = items
                        .iter()
                        .find(|item| item.id().as_ref() == "toggle-pause")
                        .and_then(|item| item.as_menuitem())
                        .expect("recording control");
                    assert_eq!(pause.is_enabled(), monitoring);
                }
            }
        }
        Ok(())
    }
}
