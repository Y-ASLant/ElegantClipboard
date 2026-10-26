pub use super::shortcut::{
    normalize_main_shortcut, normalize_paste_shortcut, validate_paste_shortcuts,
};
use super::{shortcut::parse, x11::X11};
use anyhow::{Context, Result, bail};
use async_channel::Sender;
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use x11rb::{
    connection::Connection,
    protocol::{Event, xproto::*},
};

struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    errors: async_channel::Receiver<String>,
}

impl Worker {
    fn start(
        x: X11,
        bindings: Vec<(u8, u16, u8, bool)>,
        locks: &[u16],
        dispatch: impl Fn(u8, bool, (isize, u32)) + Send + 'static,
    ) -> Result<Self> {
        let ignored_locks = locks.iter().fold(0, |mask, lock| mask | lock);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (error_sender, errors) = async_channel::bounded(1);
        let thread = thread::Builder::new()
            .name("x11-global-shortcuts".into())
            .spawn(move || {
                let mut pressed = HashSet::new();
                let result = (|| -> Result<()> {
                    while !stopping.load(Ordering::Acquire) {
                        while let Some(event) = x.connection.poll_for_event()? {
                            if let Event::KeyPress(event) = event
                                && pressed.insert(event.detail)
                                && let Some((_, _, slot, favorite)) =
                                    bindings.iter().find(|(key, modifiers, _, _)| {
                                        *key == event.detail
                                            && shortcut_modifiers(
                                                u16::from(event.state),
                                                ignored_locks,
                                            ) == *modifiers
                                    })
                            {
                                let target = x.foreground().unwrap_or((0, 0));
                                dispatch(*slot, *favorite, target);
                            }
                        }
                        if !pressed.is_empty() {
                            let state = x.connection.query_keymap()?.reply()?.keys;
                            pressed.retain(|key| {
                                state[usize::from(*key / 8)] & (1 << (*key % 8)) != 0
                            });
                        }
                        thread::sleep(Duration::from_millis(8));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let message = format!("X11 global shortcut listener stopped: {error:#}");
                    eprintln!("{message}");
                    let _ = error_sender.try_send(message);
                }
                // Dropping this dedicated connection releases every passive grab.
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
            errors,
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn lock_masks(x: &X11) -> Result<Vec<u16>> {
    let mut ignored = 2u16; // Caps Lock
    let mapping = x.connection.get_modifier_mapping()?.reply()?;
    let width = mapping.keycodes.len() / 8;
    for symbol in [0xff7f, 0xff14] {
        // Num Lock and Scroll Lock
        if let Ok(key) = x.keycode(symbol) {
            for (index, keys) in mapping.keycodes.chunks(width.max(1)).enumerate() {
                if keys.contains(&key) {
                    ignored |= 1 << index;
                }
            }
        }
    }
    let mut masks = vec![0];
    for bit in 0..8 {
        if ignored & (1 << bit) != 0 {
            let count = masks.len();
            for index in 0..count {
                masks.push(masks[index] | (1 << bit));
            }
        }
    }
    Ok(masks)
}

fn shortcut_modifiers(state: u16, ignored_locks: u16) -> u16 {
    // Core event state also contains mouse-button bits. Preserve every actual
    // keyboard modifier except the lock modifiers used by our passive grabs.
    state & 0xff & !ignored_locks
}

fn register(x: &X11, key: u8, modifiers: u16, locks: &[u16]) -> Result<()> {
    let mut registered = Vec::new();
    for lock in locks {
        let mask = ModMask::from(modifiers | lock);
        if let Err(error) = x
            .connection
            .grab_key(false, x.root, mask, key, GrabMode::ASYNC, GrabMode::ASYNC)?
            .check()
        {
            for mask in registered {
                let _ = x.connection.ungrab_key(key, x.root, mask);
            }
            x.connection.flush()?;
            return Err(error).context("Shortcut is already owned by another application");
        }
        registered.push(mask);
    }
    Ok(())
}

pub struct Hotkey {
    worker: Worker,
}

impl Hotkey {
    pub fn start(choice: &HotkeyPreference, events: Sender<(isize, u32)>) -> Result<Self> {
        let HotkeyPreference::Shortcut(value) = choice else {
            bail!("Global shortcut is disabled");
        };
        let parsed = parse(value, true)?;
        let x = X11::connect()?;
        let key = x.keycode(parsed.key)?;
        let locks = lock_masks(&x)?;
        register(&x, key, parsed.modifiers, &locks)?;
        let worker = Worker::start(
            x,
            vec![(key, parsed.modifiers, 0, false)],
            &locks,
            move |_, _, target| {
                let _ = events.try_send(target);
            },
        )?;
        Ok(Self { worker })
    }

    pub fn take_error(&self) -> Option<String> {
        self.worker.errors.try_recv().ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteHotkeyEvent {
    pub slot: u8,
    pub favorite: bool,
    pub target: (isize, u32),
}

#[derive(Clone, Debug)]
pub struct PasteRegistrationWarning {
    pub slot: u8,
    pub favorite: bool,
    pub message: String,
}

impl std::fmt::Display for PasteRegistrationWarning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub struct PasteHotkeys {
    worker: Worker,
}

impl PasteHotkeys {
    pub fn start(
        events: Sender<PasteHotkeyEvent>,
        shortcuts: &PasteShortcutConfig,
        main: &HotkeyPreference,
    ) -> Result<(Self, Vec<PasteRegistrationWarning>)> {
        validate_paste_shortcuts(shortcuts, main)?;
        let x = X11::connect()?;
        let locks = lock_masks(&x)?;
        let mut bindings = Vec::new();
        let mut warnings = Vec::new();
        for favorite in [false, true] {
            for slot in 1..=10 {
                let value = shortcuts.slot(favorite, slot).unwrap_or_default();
                if value.is_empty() {
                    continue;
                }
                let parsed = parse(value, false)?;
                for symbol in std::iter::once(parsed.key).chain(parsed.numpad) {
                    match x.keycode(symbol).and_then(|key| {
                        register(&x, key, parsed.modifiers, &locks)?;
                        Ok(key)
                    }) {
                        Ok(key) => bindings.push((key, parsed.modifiers, slot, favorite)),
                        Err(error) => warnings.push(PasteRegistrationWarning {
                            slot,
                            favorite,
                            message: format!("{value}: {error:#}"),
                        }),
                    }
                }
            }
        }
        if bindings.is_empty() && !warnings.is_empty() {
            bail!(
                "No quick-paste shortcut could be registered: {}",
                warnings[0]
            );
        }
        let worker = Worker::start(x, bindings, &locks, move |slot, favorite, target| {
            let _ = events.try_send(PasteHotkeyEvent {
                slot,
                favorite,
                target,
            });
        })?;
        Ok((Self { worker }, warnings))
    }

    pub fn take_error(&self) -> Option<String> {
        self.worker.errors.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::shortcut_modifiers;

    #[test]
    fn actual_lock_mapping_is_ignored_without_discarding_other_modifiers() {
        let locks = 2 | 16 | 128; // CapsLock, NumLock, ScrollLock on Mod5.
        for state in [8, 8 | 2, 8 | 16 | 128, 8 | locks | 256] {
            assert_eq!(shortcut_modifiers(state, locks), 8);
        }
        assert_eq!(shortcut_modifiers(8 | 4 | locks, locks), 8 | 4);
        assert_eq!(shortcut_modifiers(8 | 32 | locks, locks), 8 | 32);
        // Mod5 is not ignored on a layout where it is AltGr, not ScrollLock.
        assert_eq!(shortcut_modifiers(8 | 128, 2 | 16), 8 | 128);
    }
}
