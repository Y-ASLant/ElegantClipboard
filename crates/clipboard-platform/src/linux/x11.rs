use anyhow::{Context, Result, bail};
use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt, rust_connection::RustConnection,
};

pub(crate) struct X11 {
    pub connection: RustConnection,
    pub root: u32,
}

pub fn unavailable_reason() -> Option<&'static str> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|value| value == "wayland")
    {
        Some(
            "Wayland does not permit global window inspection, synthetic paste, or X11 global shortcuts. Use an X11 session for these features.",
        )
    } else if std::env::var_os("DISPLAY").is_none() {
        Some("No X11 display is available.")
    } else {
        None
    }
}

impl X11 {
    pub fn connect() -> Result<Self> {
        if let Some(reason) = unavailable_reason() {
            bail!(reason);
        }
        let (connection, screen) =
            x11rb::connect(None).context("Cannot connect to the X11 display")?;
        let root = connection.setup().roots[screen].root;
        Ok(Self { connection, root })
    }

    pub fn atom(&self, name: &str) -> Result<u32> {
        Ok(self
            .connection
            .intern_atom(false, name.as_bytes())?
            .reply()?
            .atom)
    }

    pub fn property(&self, window: u32, name: &str) -> Result<Vec<u32>> {
        let reply = self
            .connection
            .get_property(false, window, self.atom(name)?, 0u32, 0, 1_048_576)?
            .reply()?;
        Ok(reply
            .value32()
            .map(|values| values.collect())
            .unwrap_or_default())
    }

    pub fn pid(&self, window: u32) -> Result<u32> {
        self.property(window, "_NET_WM_PID")?
            .first()
            .copied()
            .context("Window has no process identifier")
    }

    pub fn foreground(&self) -> Result<(isize, u32)> {
        let window = self
            .property(self.root, "_NET_ACTIVE_WINDOW")?
            .first()
            .copied()
            .filter(|id| *id != 0)
            .context("The window manager does not report an active window")?;
        Ok((window as isize, self.pid(window)?))
    }

    pub fn keycode(&self, keysym: u32) -> Result<u8> {
        let setup = self.connection.setup();
        let first = setup.min_keycode;
        let mapping = self
            .connection
            .get_keyboard_mapping(first, setup.max_keycode - first + 1)?
            .reply()?;
        mapping
            .keysyms
            .chunks(usize::from(mapping.keysyms_per_keycode))
            .position(|symbols| symbols.contains(&keysym))
            .map(|offset| first + offset as u8)
            .context("Shortcut key is not present in the current X11 keyboard layout")
    }
}
