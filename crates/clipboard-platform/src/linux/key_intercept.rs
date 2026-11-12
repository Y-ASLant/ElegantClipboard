//! X11 passive key grabs that forward navigation keys to the history window
//! while it is shown without input focus, so the paste target keeps focus.
use super::x11::X11;
pub use crate::key_intercept_types::InterceptedKey;
use crate::key_intercept_types::{NAVIGATION_KEYS, control_shortcut};
use anyhow::{Result, bail};
use async_channel::Sender;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{
            ChangeWindowAttributesAux, ConnectionExt as _, EventMask, GrabMode, KEY_PRESS_EVENT,
            KEY_RELEASE_EVENT, MapState, ModMask, NotifyDetail,
        },
        xtest::ConnectionExt as _,
    },
};

/// Keysyms in `NAVIGATION_KEYS` order.
const NAVIGATION_KEYSYMS: [u32; 11] = [
    0xff52, 0xff54, 0xff51, 0xff53, 0xff50, 0xff57, 0xff55, 0xff56, 0xff0d, 0xffff, 0xff1b,
];
const KEYPAD_ENTER: u32 = 0xff8d;
const SHIFT: u16 = 1;
const CONTROL: u16 = 4;

/// Text keycodes swallowed before the window took focus, replayed in order.
static PENDING_TEXT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
/// Set while replaying so the worker releases its grabs before keys are injected.
static SUSPENDED: AtomicBool = AtomicBool::new(false);
static GRABBED: AtomicBool = AtomicBool::new(false);

fn navigation_key(keysym: u32) -> Option<&'static str> {
    if keysym == KEYPAD_ENTER {
        return Some("enter");
    }
    NAVIGATION_KEYSYMS
        .iter()
        .position(|symbol| *symbol == keysym)
        .map(|index| NAVIGATION_KEYS[index])
}

/// Latin-1 keysyms in the printable ASCII range equal their character code.
fn is_printable(keysym: u32) -> bool {
    (0x20..0x7f).contains(&keysym)
}

fn classify(keysym: u32, state: u16) -> Option<InterceptedKey> {
    let control = state & CONTROL != 0;
    let shift = state & SHIFT != 0;
    if let Some(key) = navigation_key(keysym) {
        return Some(InterceptedKey::Shortcut {
            key,
            control,
            shift,
        });
    }
    if !is_printable(keysym) {
        return None;
    }
    if control {
        let key = char::from_u32(keysym).and_then(control_shortcut)?;
        return Some(InterceptedKey::Shortcut {
            key,
            control,
            shift,
        });
    }
    Some(InterceptedKey::Text)
}

/// (keycode, base keysym, modifier sets to grab) for every forwarded key.
fn grab_plan(x: &X11) -> Result<Vec<(u8, u32, &'static [u16])>> {
    let setup = x.connection.setup();
    let first = setup.min_keycode;
    let mapping = x
        .connection
        .get_keyboard_mapping(first, setup.max_keycode - first + 1)?
        .reply()?;
    let per_keycode = usize::from(mapping.keysyms_per_keycode).max(1);
    Ok(mapping
        .keysyms
        .chunks(per_keycode)
        .enumerate()
        .filter_map(|(offset, symbols)| {
            let keysym = *symbols.first()?;
            let modifiers: &'static [u16] = if navigation_key(keysym).is_some() {
                &[0, SHIFT, CONTROL, CONTROL | SHIFT]
            } else if !is_printable(keysym) {
                return None;
            } else if char::from_u32(keysym).and_then(control_shortcut).is_some() {
                &[0, SHIFT, CONTROL]
            } else {
                &[0, SHIFT]
            };
            Some((first + offset as u8, keysym, modifiers))
        })
        .collect())
}

fn set_grabs(x: &X11, plan: &[(u8, u32, &'static [u16])], locks: &[u16], grab: bool) -> Result<()> {
    if grab {
        for (keycode, _, modifiers) in plan {
            for modifier in *modifiers {
                for lock in locks {
                    // Unchecked: a key another client already grabbed stays with it.
                    let _ = x.connection.grab_key(
                        false,
                        x.root,
                        ModMask::from(modifier | lock),
                        *keycode,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )?;
                }
            }
        }
    } else {
        x.connection.ungrab_key(0, x.root, ModMask::ANY)?;
    }
    x.connection.flush()?;
    GRABBED.store(grab, Ordering::Release);
    Ok(())
}

fn is_viewable(x: &X11, window: u32) -> Result<bool> {
    Ok(x.connection
        .get_window_attributes(window)?
        .reply()?
        .map_state
        == MapState::VIEWABLE)
}

fn focus_is_ours(x: &X11, window: u32) -> Result<bool> {
    let focus = x.connection.get_input_focus()?.reply()?.focus;
    Ok(focus == window || x.pid(focus).is_ok_and(|pid| pid == std::process::id()))
}

/// Replay swallowed text keys after the history window has taken input focus.
pub fn replay_pending_text() -> Result<()> {
    SUSPENDED.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_millis(300);
    while GRABBED.load(Ordering::Acquire) {
        if Instant::now() > deadline {
            bail!("Key forwarding did not release its grabs");
        }
        thread::sleep(Duration::from_millis(5));
    }
    let keys = std::mem::take(&mut *PENDING_TEXT.lock().unwrap_or_else(|e| e.into_inner()));
    let x = X11::connect()?;
    x.connection.xtest_get_version(2, 2)?.reply()?;
    for key in keys {
        for kind in [KEY_PRESS_EVENT, KEY_RELEASE_EVENT] {
            x.connection
                .xtest_fake_input(kind, key, 0, x.root, 0, 0, 0)?
                .check()?;
        }
    }
    x.connection.flush()?;
    Ok(())
}

/// Drop swallowed text keys when the window could not take focus.
pub fn discard_pending_text() {
    PENDING_TEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

pub struct KeyInterceptor {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl KeyInterceptor {
    pub fn start(events: Sender<InterceptedKey>, window_handle: isize) -> Result<Self> {
        let x = X11::connect_events()?;
        let window = window_handle as u32;
        x.connection
            .change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::STRUCTURE_NOTIFY | EventMask::FOCUS_CHANGE),
            )?
            .check()?;
        let plan = grab_plan(&x)?;
        let locks = super::hotkey::lock_masks(&x)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::Builder::new()
            .name("x11-key-intercept".into())
            .spawn(move || {
                let mut grabbed = false;
                let result = (|| -> Result<()> {
                    // Map and focus changes arrive as events; no per-tick X round trips.
                    let mut viewable = is_viewable(&x, window)?;
                    let mut focused = focus_is_ours(&x, window)?;
                    while !stopping.load(Ordering::Acquire) {
                        while let Some(event) = x.connection.poll_for_event()? {
                            let event = match event {
                                Event::KeyPress(event) => event,
                                Event::MapNotify(event) if event.window == window => {
                                    viewable = true;
                                    continue;
                                }
                                Event::UnmapNotify(event) if event.window == window => {
                                    viewable = false;
                                    SUSPENDED.store(false, Ordering::Release);
                                    continue;
                                }
                                Event::FocusIn(event) if event.event == window => {
                                    focused = true;
                                    continue;
                                }
                                Event::FocusOut(event)
                                    if event.event == window
                                        && event.detail != NotifyDetail::INFERIOR =>
                                {
                                    focused = false;
                                    continue;
                                }
                                _ => continue,
                            };
                            let Some((_, keysym, _)) =
                                plan.iter().find(|(keycode, _, _)| *keycode == event.detail)
                            else {
                                continue;
                            };
                            let Some(key) = classify(*keysym, u16::from(event.state)) else {
                                continue;
                            };
                            if key == InterceptedKey::Text {
                                PENDING_TEXT
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner())
                                    .push(event.detail);
                            }
                            let _ = events.try_send(key);
                        }
                        let want = viewable && !focused && !SUSPENDED.load(Ordering::Acquire);
                        if want != grabbed {
                            set_grabs(&x, &plan, &locks, want)?;
                            grabbed = want;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("X11 key forwarding stopped: {error:#}");
                }
                GRABBED.store(false, Ordering::Release);
                // Dropping this dedicated connection releases every passive grab.
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for KeyInterceptor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
