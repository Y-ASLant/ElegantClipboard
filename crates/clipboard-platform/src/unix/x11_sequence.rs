use anyhow::{Context, Result, anyhow};
use std::sync::Mutex;
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xfixes::{ConnectionExt as _, SelectionEventMask},
        xproto::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

struct SelectionClock {
    connection: RustConnection,
    generation: u32,
}

impl SelectionClock {
    fn new() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None).context("无法连接 X11 剪贴板服务")?;
        connection
            .xfixes_query_version(5, 0)?
            .reply()
            .context("X11 服务器不支持 XFixes 剪贴板监听")?;
        let clipboard = connection.intern_atom(false, b"CLIPBOARD")?.reply()?.atom;
        connection
            .xfixes_select_selection_input(
                connection.setup().roots[screen].root,
                clipboard,
                SelectionEventMask::SET_SELECTION_OWNER
                    | SelectionEventMask::SELECTION_CLIENT_CLOSE
                    | SelectionEventMask::SELECTION_WINDOW_DESTROY,
            )?
            .check()?;
        Ok(Self {
            connection,
            generation: 1,
        })
    }

    fn current(&mut self) -> Result<u32> {
        // A reply flushes all selection notifications queued before this request,
        // including writes made through clipboard-rs on its separate connection.
        self.connection.get_input_focus()?.reply()?;
        while let Some(event) = self.connection.poll_for_event()? {
            if matches!(event, Event::XfixesSelectionNotify(_)) {
                self.generation = self.generation.wrapping_add(1).max(1);
            }
        }
        Ok(self.generation)
    }
}

pub(super) fn current() -> Result<u32> {
    static CLOCK: Mutex<Option<SelectionClock>> = Mutex::new(None);
    let mut clock = CLOCK
        .lock()
        .map_err(|_| anyhow!("X11 剪贴板序列状态异常"))?;
    if clock.is_none() {
        *clock = Some(SelectionClock::new()?);
    }
    clock
        .as_mut()
        .expect("selection clock initialized")
        .current()
}
