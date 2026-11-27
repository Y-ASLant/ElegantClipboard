use gpui_kit::{Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::{
    connection::Connection,
    properties::WmHints,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt, EventMask, MapState,
        PropMode, StackMode,
    },
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
    map_panel(&connection, id as u32);
}

fn map_panel(connection: &impl Connection, id: u32) {
    let _ = set_input_hint(connection, id, false);
    // EWMH: user time 0 asks the window manager not to focus the window on map.
    if let Some(user_time) = intern(connection, "_NET_WM_USER_TIME") {
        let _ = connection.change_property32(
            PropMode::REPLACE,
            id,
            user_time,
            AtomEnum::CARDINAL,
            &[0],
        );
    }
    let _ = request_above(connection, id, true, false);
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

/// No-op on Linux: X11 click activation is governed by the WM input hint.
pub fn set_click_activate_bounds(_window: &Window, _bounds: Option<Bounds<Pixels>>) {}

/// Keep or drop `_NET_WM_STATE_ABOVE` without asking for focus.
pub fn set_above(window: &Window, above: bool) {
    let _ = update_above(window, above, false);
}

#[derive(Clone, Copy)]
pub struct MainWindowLevel(());

pub fn lower_for_editor(window: &Window) -> Option<MainWindowLevel> {
    update_above(window, false, true).map(|()| MainWindowLevel(()))
}

pub fn restore_after_editor(window: &Window, _: MainWindowLevel) {
    set_above(window, true);
}

fn update_above(window: &Window, above: bool, require_present: bool) -> Option<()> {
    let id = window_id(window)? as u32;
    let (connection, _) = x11rb::connect(None).ok()?;
    request_above(&connection, id, above, require_present)
}

fn request_above(
    connection: &impl Connection,
    id: u32,
    above: bool,
    require_present: bool,
) -> Option<()> {
    let state = intern(connection, "_NET_WM_STATE")?;
    let flag = intern(connection, "_NET_WM_STATE_ABOVE")?;
    let attributes = connection.get_window_attributes(id).ok()?.reply().ok()?;
    let managed = attributes.map_state != MapState::UNMAPPED && !attributes.override_redirect;
    if require_present || !managed {
        let reply = connection
            .get_property(false, id, state, AtomEnum::ATOM, 0, u32::MAX)
            .ok()?
            .reply()
            .ok()?;
        let present = reply
            .value32()
            .is_some_and(|mut atoms| atoms.any(|atom| atom == flag));
        if present == above {
            return None;
        }
        if !managed {
            // Unmanaged windows supply their desired state through the property.
            let cookie = if above {
                connection.change_property32(PropMode::APPEND, id, state, AtomEnum::ATOM, &[flag])
            } else {
                let remaining: Vec<_> = reply.value32()?.filter(|atom| *atom != flag).collect();
                connection.change_property32(
                    PropMode::REPLACE,
                    id,
                    state,
                    AtomEnum::ATOM,
                    &remaining,
                )
            };
            return cookie.ok()?.check().ok();
        }
    }
    // For managed windows only the WM edits the state property. An unconditional add
    // on restore also follows a pending remove if the editor closes immediately.
    let root = connection.query_tree(id).ok()?.reply().ok()?.root;
    let event = ClientMessageEvent::new(32, id, state, [u32::from(above), flag, 0, 1, 0]);
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .ok()?
        .check()
        .ok()
}

/// Place the window above its siblings without asking for focus.
pub fn raise_front(window: &Window) {
    let Some(id) = window_id(window) else {
        return;
    };
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let id = id as u32;
    let _ =
        connection.configure_window(id, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
    let _ = connection.map_window(id);
    let _ = connection.flush();
}

fn intern(connection: &impl Connection, name: &str) -> Option<u32> {
    connection
        .intern_atom(false, name.as_bytes())
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| reply.atom)
}

fn set_input_hint(connection: &impl Connection, id: u32, input: bool) -> anyhow::Result<()> {
    let mut hints = WmHints::get(connection, id)?.reply()?.unwrap_or_default();
    hints.input = Some(input);
    hints.set(connection, id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};

    #[test]
    #[ignore = "requires a private Xvfb server"]
    fn x11_panel_show_preserves_other_window_states() -> anyhow::Result<()> {
        anyhow::ensure!(std::env::var("ELEGANT_X11_TEST_SERVER").as_deref() == Ok("private-xvfb"));
        let (connection, screen) = x11rb::connect(None)?;
        let id = connection.generate_id()?;
        let root = connection.setup().roots[screen].root;
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                id,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::default(),
            )?
            .check()?;
        let state = intern(&connection, "_NET_WM_STATE").unwrap();
        let skip = intern(&connection, "_NET_WM_STATE_SKIP_TASKBAR").unwrap();
        let above = intern(&connection, "_NET_WM_STATE_ABOVE").unwrap();
        connection
            .change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &[skip])?
            .check()?;
        for _ in 0..2 {
            map_panel(&connection, id);
            let reply = connection
                .get_property(false, id, state, AtomEnum::ATOM, 0, u32::MAX)?
                .reply()?;
            assert_eq!(reply.value32().unwrap().collect::<Vec<_>>(), [skip, above]);
            connection.unmap_window(id)?.check()?;
        }
        Ok(())
    }
}
