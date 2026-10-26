use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{
    connection::Connection,
    protocol::xproto::{ConnectionExt, MapState},
};

pub fn window_id(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Xlib(handle) => Some(handle.window as isize),
        RawWindowHandle::Xcb(handle) => Some(handle.window.get() as isize),
        _ => None,
    }
}

pub fn set_window_visible(window: &Window, visible: bool) {
    if let Some(id) = window_id(window) {
        if let Ok((connection, _)) = x11rb::connect(None) {
            if visible {
                let _ = connection.map_window(id as u32);
            } else {
                let _ = connection.unmap_window(id as u32);
            }
            let _ = connection.flush();
        }
        if visible {
            window.activate_window();
        }
    } else if visible {
        window.activate_window();
    } else {
        // Wayland intentionally allows minimize, but not arbitrary unmapping.
        window.minimize_window();
    }
}

pub fn is_window_shown(window: &Window) -> bool {
    let Some(id) = window_id(window) else {
        return window.is_visible();
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return false;
    };
    connection
        .get_window_attributes(id as u32)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|attributes| attributes.map_state == MapState::VIEWABLE)
}

pub fn is_window_minimized(window: &Window) -> bool {
    !is_window_shown(window)
}

pub fn foreground_window_is_ours() -> bool {
    clipboard_platform::desktop::foreground_target()
        .is_some_and(|target| target.1 == std::process::id())
}
