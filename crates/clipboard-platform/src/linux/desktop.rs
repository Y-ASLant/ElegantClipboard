use super::x11::X11;
pub use super::x11::unavailable_reason;
use anyhow::{Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, xtest::ConnectionExt as _},
};

pub fn x11_available() -> bool {
    X11::connect().is_ok()
}

pub fn foreground_target() -> Option<(isize, u32)> {
    X11::connect().ok()?.foreground().ok()
}

pub fn target_alive(target: (isize, u32)) -> bool {
    X11::connect()
        .and_then(|x| x.pid(target.0 as u32))
        .is_ok_and(|pid| pid == target.1)
}

pub fn activate_target(target: (isize, u32)) -> Result<()> {
    let x = X11::connect()?;
    if x.pid(target.0 as u32)? != target.1 {
        bail!("The paste target no longer exists");
    }
    let event = ClientMessageEvent::new(
        32,
        target.0 as u32,
        x.atom("_NET_ACTIVE_WINDOW")?,
        [2, 0, 0, 0, 0],
    );
    x.connection
        .send_event(
            false,
            x.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )?
        .check()?;
    x.connection.flush()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while std::time::Instant::now() < deadline {
        if x.foreground().is_ok_and(|active| active == target) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    bail!("The window manager refused to activate the paste target")
}

pub fn send_paste(preference: PasteKeyPreference) -> Result<()> {
    let x = X11::connect()?;
    x.connection.xtest_get_version(2, 2)?.reply()?;
    if modifiers(&x)? & 0xcd != 0 {
        bail!("Release shortcut modifiers before pasting");
    }
    let (modifier, key) = match preference {
        PasteKeyPreference::CtrlV => (0xffe3, u32::from(b'v')),
        PasteKeyPreference::ShiftInsert => (0xffe1, 0xff63),
    };
    let modifier = x.keycode(modifier)?;
    let key = x.keycode(key)?;
    let result = (|| -> Result<()> {
        x.connection
            .xtest_fake_input(KEY_PRESS_EVENT, modifier, 0, x.root, 0, 0, 0)?
            .check()?;
        x.connection
            .xtest_fake_input(KEY_PRESS_EVENT, key, 0, x.root, 0, 0, 0)?
            .check()?;
        Ok(())
    })();
    // Always release our synthetic keys, including after a failed press.
    let release_key = x
        .connection
        .xtest_fake_input(KEY_RELEASE_EVENT, key, 0, x.root, 0, 0, 0)?
        .check();
    let release_modifier = x
        .connection
        .xtest_fake_input(KEY_RELEASE_EVENT, modifier, 0, x.root, 0, 0, 0)?
        .check();
    x.connection.flush()?;
    result?;
    release_key?;
    release_modifier?;
    Ok(())
}

fn modifiers(x: &X11) -> Result<u16> {
    Ok(u16::from(x.connection.query_pointer(x.root)?.reply()?.mask))
}

pub fn modifiers_pressed() -> bool {
    X11::connect()
        .and_then(|x| modifiers(&x))
        .map_or(true, |mask| mask & 0xcd != 0)
}

pub fn shift_pressed() -> bool {
    X11::connect()
        .and_then(|x| modifiers(&x))
        .is_ok_and(|mask| mask & 1 != 0)
}
