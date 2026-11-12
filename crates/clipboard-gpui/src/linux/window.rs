use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{
    connection::Connection,
    properties::WmHints,
    protocol::xproto::{AtomEnum, ConnectionExt, MapState, PropMode},
    wrapper::ConnectionExt as _,
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

/// Map above other windows without taking input focus from the paste target.
/// Wayland compositors own focus, so they keep normal activation.
pub fn show_without_activation(window: &Window) {
    let Some(id) = window_id(window) else {
        window.activate_window();
        return;
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let id = id as u32;
    let _ = set_input_hint(&connection, id, false);
    let atom = |name: &str| {
        connection
            .intern_atom(false, name.as_bytes())
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom)
    };
    // EWMH: user time 0 asks the window manager not to focus the window on map.
    if let Some(user_time) = atom("_NET_WM_USER_TIME") {
        let _ = connection.change_property32(
            PropMode::REPLACE,
            id,
            user_time,
            AtomEnum::CARDINAL,
            &[0],
        );
    }
    if let (Some(state), Some(above)) = (atom("_NET_WM_STATE"), atom("_NET_WM_STATE_ABOVE")) {
        let _ =
            connection.change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &[above]);
    }
    let _ = connection.map_window(id);
    let _ = connection.flush();
}

/// Accept input focus again (text input needs it) and ask for activation.
pub fn activate_for_input(window: &Window) {
    if let Some(id) = window_id(window)
        && let Ok((connection, _)) = x11rb::connect(None)
    {
        let _ = set_input_hint(&connection, id as u32, true);
        let _ = connection.flush();
    }
    window.activate_window();
}

fn set_input_hint(connection: &impl Connection, id: u32, input: bool) -> anyhow::Result<()> {
    let mut hints = WmHints::get(connection, id)?.reply()?.unwrap_or_default();
    hints.input = Some(input);
    hints.set(connection, id)?;
    Ok(())
}
