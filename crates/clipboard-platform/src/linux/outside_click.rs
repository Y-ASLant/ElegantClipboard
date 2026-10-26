use super::x11::X11;
use anyhow::{Result, bail};
use async_channel::Sender;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xinput::{self, ConnectionExt as _},
        xproto::{ConnectionExt as _, MapState},
    },
};

pub struct OutsideClickMonitor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl OutsideClickMonitor {
    pub fn start(events: Sender<(i32, i32)>, window_handle: isize) -> Result<Self> {
        let x = X11::connect()?;
        let version = x.connection.xinput_xi_query_version(2, 0)?.reply()?;
        if version.major_version < 2 {
            bail!("Outside-click monitoring requires XInput 2");
        }
        x.connection
            .get_window_attributes(window_handle as u32)?
            .reply()?;
        x.connection
            .xinput_xi_select_events(
                x.root,
                &[xinput::EventMask {
                    deviceid: 1,
                    mask: vec![xinput::XIEventMask::RAW_BUTTON_PRESS],
                }],
            )?
            .check()?;
        x.connection.flush()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("x11-outside-click".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    while !stopping.load(Ordering::Acquire) {
                        while let Some(event) = x.connection.poll_for_event()? {
                            if let Event::XinputRawButtonPress(event) = event
                                && matches!(event.detail, 1 | 3)
                                && x.connection
                                    .get_window_attributes(window_handle as u32)?
                                    .reply()?
                                    .map_state
                                    == MapState::VIEWABLE
                            {
                                let pointer = x.connection.query_pointer(x.root)?.reply()?;
                                let mut child = pointer.child;
                                let mut ours = false;
                                while child != 0 {
                                    if child == window_handle as u32
                                        || x.pid(child).is_ok_and(|pid| pid == std::process::id())
                                    {
                                        ours = true;
                                        break;
                                    }
                                    child = x.connection.query_pointer(child)?.reply()?.child;
                                }
                                if !ours {
                                    let _ = events.try_send((
                                        i32::from(pointer.root_x),
                                        i32::from(pointer.root_y),
                                    ));
                                }
                            }
                        }
                        thread::sleep(Duration::from_millis(8));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("Outside-click monitor stopped: {error:#}");
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for OutsideClickMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
