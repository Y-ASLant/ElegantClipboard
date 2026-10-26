use anyhow::{Context, Result, bail};
use std::sync::{Arc, LazyLock, Mutex};
use x11rb::{
    connection::Connection, protocol::xproto::ConnectionExt, rust_connection::RustConnection,
};

pub(crate) struct X11 {
    pub connection: Arc<RustConnection>,
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
    /// 桌面功能（粘贴等待、拖拽循环、前台查询）每秒会调用数十次；
    /// 每次都新建 socket + setup 握手的开销过高，因此进程内复用一条连接
    /// （调用方短暂持有 Arc；仅在缓存记录上次连接失败时跳过重复重试）。
    pub fn connect() -> Result<Self> {
        type ConnectionResult = Result<(Arc<RustConnection>, u32), String>;
        static SHARED: LazyLock<Mutex<Option<ConnectionResult>>> =
            LazyLock::new(|| Mutex::new(None));
        let mut shared = SHARED.lock().unwrap_or_else(|error| error.into_inner());
        match shared.as_ref() {
            Some(Ok((connection, root))) => Ok(Self {
                connection: Arc::clone(connection),
                root: *root,
            }),
            Some(Err(reason)) => bail!("{reason}"),
            None => {
                let established = Self::establish().map_err(|error| error.to_string());
                match established {
                    Ok(x11) => {
                        let shared_entry = (Arc::clone(&x11.connection), x11.root);
                        *shared = Some(Ok(shared_entry));
                        Ok(x11)
                    }
                    Err(reason) => {
                        *shared = Some(Err(reason.clone()));
                        bail!("{reason}")
                    }
                }
            }
        }
    }

    fn establish() -> Result<Self> {
        if let Some(reason) = unavailable_reason() {
            bail!(reason);
        }
        let (connection, screen) =
            x11rb::connect(None).context("Cannot connect to the X11 display")?;
        let root = connection.setup().roots[screen].root;
        Ok(Self {
            connection: Arc::new(connection),
            root,
        })
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
        // 键盘映射极少变化；xdnd 主循环、快捷键注册与粘贴路径会反复查询，
        // 用 (min_keycode, 映射) 的内容寻址缓存避免每次拉取约 6KB 的回复。
        type KeyboardMapping = (u8, u8, Vec<u32>);
        static CACHE: LazyLock<Mutex<Option<KeyboardMapping>>> = LazyLock::new(|| Mutex::new(None));
        let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
        if cache.is_none() {
            let setup = self.connection.setup();
            let first = setup.min_keycode;
            let mapping = self
                .connection
                .get_keyboard_mapping(first, setup.max_keycode - first + 1)?
                .reply()?;
            *cache = Some((first, mapping.keysyms_per_keycode, mapping.keysyms));
        }
        let (first, per_keycode, keysyms) = cache.as_ref().expect("cache filled above");
        keysyms
            .chunks(usize::from(*per_keycode))
            .position(|symbols| symbols.contains(&keysym))
            .map(|offset| first + offset as u8)
            .context("Shortcut key is not present in the current X11 keyboard layout")
    }
}
