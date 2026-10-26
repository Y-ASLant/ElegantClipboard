pub use crate::popup_geometry::hover_popup_bounds;
use anyhow::Result;
use clipboard_core::preferences::WindowPositionPreference;
use gpui_kit::Window;

pub fn position_window(window: &Window, mode: WindowPositionPreference) -> Result<()> {
    if mode == WindowPositionPreference::FixedPosition
        || window.is_maximized()
        || window.is_fullscreen()
    {
        return Ok(());
    }
    position_native(window, mode)
}

#[cfg(target_os = "linux")]
fn position_native(window: &Window, mode: WindowPositionPreference) -> Result<()> {
    use anyhow::Context;
    use x11rb::{
        connection::Connection,
        protocol::{
            randr::ConnectionExt as _,
            xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt},
        },
    };
    let id = super::window::window_id(window)
        .context("Wayland 由合成器管理窗口位置；请使用保持位置 / Wayland controls window placement; use Keep position")? as u32;
    let (connection, screen) = x11rb::connect(None)?;
    let root = connection.setup().roots[screen].root;
    let pointer = connection.query_pointer(root)?.reply()?;
    let geometry = connection.get_geometry(id)?.reply()?;
    let monitors = connection.randr_get_monitors(root, true)?.reply()?;
    let cursor = (i32::from(pointer.root_x), i32::from(pointer.root_y));
    let monitor = monitors
        .monitors
        .iter()
        .find(|monitor| {
            cursor.0 >= i32::from(monitor.x)
                && cursor.1 >= i32::from(monitor.y)
                && cursor.0 < i32::from(monitor.x) + i32::from(monitor.width)
                && cursor.1 < i32::from(monitor.y) + i32::from(monitor.height)
        })
        .context("无法确定光标所在显示器")?;
    let mut left = i32::from(monitor.x);
    let mut top = i32::from(monitor.y);
    let mut right = left + i32::from(monitor.width);
    let mut bottom = top + i32::from(monitor.height);
    let work_atom = connection
        .intern_atom(false, b"_NET_WORKAREA")?
        .reply()?
        .atom;
    let desktop_atom = connection
        .intern_atom(false, b"_NET_CURRENT_DESKTOP")?
        .reply()?
        .atom;
    let desktop = connection
        .get_property(false, root, desktop_atom, AtomEnum::CARDINAL, 0, 1)?
        .reply()?
        .value32()
        .and_then(|mut values| values.next())
        .unwrap_or(0);
    let work = connection
        .get_property(false, root, work_atom, AtomEnum::CARDINAL, desktop * 4, 4)?
        .reply()?;
    if let Some(mut values) = work.value32()
        && let (Some(x), Some(y), Some(width), Some(height)) =
            (values.next(), values.next(), values.next(), values.next())
    {
        left = left.max(x as i32);
        top = top.max(y as i32);
        right = right.min((x as i32).saturating_add(width as i32));
        bottom = bottom.min((y as i32).saturating_add(height as i32));
    }
    let width = i32::from(geometry.width);
    let height = i32::from(geometry.height);
    let (x, y) = if mode == WindowPositionPreference::ScreenCenter {
        (
            left + (right - left - width) / 2,
            top + (bottom - top - height) / 2,
        )
    } else {
        let x = if cursor.0 + 12 + width > right {
            cursor.0 - width - 12
        } else {
            cursor.0 + 12
        };
        (x, cursor.1 + 12)
    };
    connection
        .configure_window(
            id,
            &ConfigureWindowAux::new()
                .x(x.clamp(left, (right - width).max(left)))
                .y(y.clamp(top, (bottom - height).max(top))),
        )?
        .check()?;
    connection.flush()?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn position_native(window: &Window, mode: WindowPositionPreference) -> Result<()> {
    use anyhow::Context;
    use objc2::{class, msg_send, runtime::AnyObject};
    use objc2_foundation::{NSPoint, NSRect};
    let native =
        super::window::window_id(window).context("无法取得 AppKit 窗口")? as *mut AnyObject;
    unsafe {
        let cursor: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
        let count: usize = msg_send![screens, count];
        let mut selected = None;
        for index in 0..count {
            let screen: *mut AnyObject = msg_send![screens, objectAtIndex: index];
            let frame: NSRect = msg_send![screen, frame];
            if cursor.x >= frame.origin.x
                && cursor.y >= frame.origin.y
                && cursor.x < frame.origin.x + frame.size.width
                && cursor.y < frame.origin.y + frame.size.height
            {
                selected = Some(screen);
                break;
            }
        }
        let screen = selected.context("无法确定光标所在显示器")?;
        let work: NSRect = msg_send![screen, visibleFrame];
        let frame: NSRect = msg_send![native, frame];
        let right = work.origin.x + work.size.width;
        let top = work.origin.y + work.size.height;
        let (x, y) = if mode == WindowPositionPreference::ScreenCenter {
            (
                work.origin.x + (work.size.width - frame.size.width) / 2.,
                work.origin.y + (work.size.height - frame.size.height) / 2.,
            )
        } else {
            let x = if cursor.x + 12. + frame.size.width > right {
                cursor.x - frame.size.width - 12.
            } else {
                cursor.x + 12.
            };
            (x, cursor.y - 12. - frame.size.height)
        };
        let origin = NSPoint::new(
            x.clamp(work.origin.x, (right - frame.size.width).max(work.origin.x)),
            y.clamp(work.origin.y, (top - frame.size.height).max(work.origin.y)),
        );
        let _: () = msg_send![native, setFrameOrigin: origin];
    }
    Ok(())
}
