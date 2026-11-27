//! Low-level keyboard hook that forwards navigation keys to the history window
//! while it is shown without activation, so the paste target keeps focus.
pub use crate::key_intercept_types::{InterceptedKey, InterceptedText};
use crate::key_intercept_types::{NAVIGATION_KEYS, control_shortcut};
use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use std::{
    cell::RefCell,
    sync::{Mutex, mpsc},
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyState, GetKeyboardLayout, INPUT, INPUT_0, INPUT_KEYBOARD,
            KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE, MAPVK_VK_TO_CHAR, MapVirtualKeyExW, SendInput, ToUnicodeEx,
            VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
            VK_HOME, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PACKET, VK_PRIOR, VK_RETURN, VK_RIGHT,
            VK_RWIN, VK_SHIFT, VK_SPACE, VK_UP,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HC_ACTION,
            IsWindowVisible, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
            LLKHF_LOWER_IL_INJECTED, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
            SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT,
            WM_SYSKEYDOWN,
        },
    },
};

/// Virtual keys in `NAVIGATION_KEYS` order.
const NAVIGATION_VKS: [VIRTUAL_KEY; 11] = [
    VK_UP, VK_DOWN, VK_LEFT, VK_RIGHT, VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_RETURN, VK_DELETE,
    VK_ESCAPE,
];

struct HookTarget {
    sender: Sender<InterceptedKey>,
    hwnd: isize,
    swallowed: [bool; 256],
}

thread_local! {
    static HOOK_TARGET: RefCell<Option<HookTarget>> = const { RefCell::new(None) };
}

/// Text keys swallowed before the window took focus, replayed in order.
static PENDING_TEXT: Mutex<Vec<(u16, u16, bool)>> = Mutex::new(Vec::new());

// Only our replay should bypass interception; accessibility tools can send ordinary input.
const REPLAY_MARKER: usize = 0xec1b_04ad;

fn key_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key.0)) }) < 0
}

fn external_foreground_thread() -> Option<u32> {
    let mut process_id = 0;
    let thread_id =
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process_id)) };
    (thread_id != 0 && process_id != unsafe { GetCurrentProcessId() }).then_some(thread_id)
}

/// An IME layout can defer a physical key rather than translating it. In
/// passive search there is no composition session; keep basic alphanumerics.
fn untranslated_alphanumeric(vk: u32, shift: bool, caps: bool) -> Option<u16> {
    match u8::try_from(vk).ok()? {
        key @ b'A'..=b'Z' => Some(u16::from(if shift ^ caps {
            key
        } else {
            key.to_ascii_lowercase()
        })),
        key @ b'0'..=b'9' if !shift => Some(u16::from(key)),
        key @ 0x60..=0x69 => Some(u16::from(b'0' + key - 0x60)),
        _ => None,
    }
}

fn classify(info: &KBDLLHOOKSTRUCT, foreground_thread: u32) -> Option<InterceptedKey> {
    if key_down(VK_MENU) || key_down(VK_LWIN) || key_down(VK_RWIN) {
        return None;
    }
    let vk = info.vkCode;
    let control = key_down(VK_CONTROL);
    let shift = key_down(VK_SHIFT);
    if let Some(index) = NAVIGATION_VKS.iter().position(|key| u32::from(key.0) == vk) {
        return Some(InterceptedKey::Shortcut {
            key: NAVIGATION_KEYS[index],
            control,
            shift,
        });
    }
    if vk == u32::from(VK_BACK.0) && !control {
        return Some(InterceptedKey::Shortcut {
            key: "backspace",
            control,
            shift,
        });
    }
    let layout = unsafe { GetKeyboardLayout(foreground_thread) };
    if control {
        let character = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, Some(layout)) };
        let key = char::from_u32(character).and_then(control_shortcut)?;
        return Some(InterceptedKey::Shortcut {
            key,
            control,
            shift,
        });
    }
    let unit = if vk == u32::from(VK_PACKET.0) {
        // Unicode input (e.g. an on-screen keyboard) carries one UTF-16 unit in scanCode.
        Some(info.scanCode as u16)
    } else {
        let mut state = [0; 256];
        state[usize::from(VK_SHIFT.0)] = if shift { 0x80 } else { 0 };
        state[usize::from(VK_CAPITAL.0)] =
            (unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1) as u8;
        let mut units = [0; 8];
        // Bit 2 leaves the source keyboard's dead-key/composition state untouched.
        match unsafe { ToUnicodeEx(vk, info.scanCode, &state, &mut units, 4, Some(layout)) } {
            0 => Some(untranslated_alphanumeric(
                vk,
                shift,
                state[usize::from(VK_CAPITAL.0)] != 0,
            )?),
            1 => Some(units[0]),
            // A dead key or a multi-unit translation is native text, not plain ASCII.
            _ => None,
        }
    };
    if unit.is_some_and(|u| u < 0x20 || u == 0x7f) {
        return None;
    }
    Some(InterceptedKey::Text(InterceptedText {
        keycode: vk as u16,
        scan_code: info.scanCode as u16,
        extended: info.flags.0 & LLKHF_EXTENDED.0 != 0,
        search_character: unit.filter(|&u| u <= 0x7e).map(|u| char::from(u as u8)),
        unmodified_space: vk == u32::from(VK_SPACE.0) && !shift,
        is_held: false,
    }))
}

unsafe extern "system" fn keyboard_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    let mut swallow = false;
    if code == HC_ACTION as i32 {
        let info = unsafe { *(data.0 as *const KBDLLHOOKSTRUCT) };
        let pressed = matches!(message.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if info.flags.0 & LLKHF_LOWER_IL_INJECTED.0 == 0
            && !(info.flags.0 & LLKHF_INJECTED.0 != 0 && info.dwExtraInfo == REPLAY_MARKER)
        {
            HOOK_TARGET.with(|target| {
                let mut target = target.borrow_mut();
                let Some(target) = target.as_mut() else {
                    return;
                };
                let Some(previous) = target.swallowed.get_mut(info.vkCode as usize) else {
                    return;
                };
                let is_held = std::mem::take(previous);
                if !pressed {
                    swallow = is_held;
                    return;
                }
                if !unsafe { IsWindowVisible(HWND(target.hwnd as *mut _)) }.as_bool() {
                    return;
                }
                let Some(thread_id) = external_foreground_thread() else {
                    return;
                };
                let Some(mut key) = classify(&info, thread_id) else {
                    return;
                };
                if let InterceptedKey::Text(text) = &mut key {
                    text.is_held = is_held;
                }
                swallow = target.sender.try_send(key).is_ok();
                *previous = swallow;
            });
        }
    }
    if swallow {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, message, data) }
    }
}

impl InterceptedText {
    /// Queue the key for native text input rather than handling it as an application shortcut.
    pub fn queue_for_replay(self) {
        PENDING_TEXT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((self.keycode, self.scan_code, self.extended));
    }
}

/// Replay swallowed text keys after the history window has taken focus.
pub fn replay_pending_text() -> Result<()> {
    let keys = std::mem::take(&mut *PENDING_TEXT.lock().unwrap_or_else(|e| e.into_inner()));
    let inputs: Vec<INPUT> = keys
        .into_iter()
        .flat_map(|(vk, scan, extended)| {
            let (vk, flags) = if vk == VK_PACKET.0 {
                (0, KEYEVENTF_UNICODE)
            } else if extended {
                (vk, KEYEVENTF_EXTENDEDKEY)
            } else {
                (vk, KEYBD_EVENT_FLAGS(0))
            };
            [flags, flags | KEYEVENTF_KEYUP].map(|key_flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scan,
                        dwFlags: key_flags,
                        dwExtraInfo: REPLAY_MARKER,
                        ..Default::default()
                    },
                },
            })
        })
        .collect();
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    anyhow::ensure!(sent as usize == inputs.len(), "无法转发已输入的按键");
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
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl KeyInterceptor {
    pub fn start(events: Sender<InterceptedKey>, window_handle: isize) -> Result<Self> {
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("key-intercept".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                HOOK_TARGET.with(|slot| {
                    *slot.borrow_mut() = Some(HookTarget {
                        sender: events,
                        hwnd: window_handle,
                        swallowed: [false; 256],
                    });
                });
                let hook =
                    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) };
                let Ok(hook) = hook else {
                    HOOK_TARGET.with(|slot| *slot.borrow_mut() = None);
                    let _ = ready.send(hook.map(|_| thread_id));
                    return;
                };
                let _ = ready.send(Ok(thread_id));
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {}
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                HOOK_TARGET.with(|slot| *slot.borrow_mut() = None);
            })
            .context("无法启动键盘转发线程")?;
        match receiver.recv().context("键盘转发线程未响应")? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(anyhow!("键盘转发注册失败：{error}"))
            }
        }
    }
}

impl Drop for KeyInterceptor {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::untranslated_alphanumeric;

    #[test]
    fn untranslated_keys_preserve_case_and_digits_without_guessing_layout_symbols() {
        let translated = |vk, shift, caps| {
            untranslated_alphanumeric(vk, shift, caps).and_then(|unit| char::from_u32(unit.into()))
        };
        assert_eq!(translated(0x41, false, false), Some('a'));
        assert_eq!(translated(0x41, false, true), Some('A'));
        assert_eq!(translated(0x41, true, true), Some('a'));
        assert_eq!(translated(0x37, false, false), Some('7'));
        assert_eq!(translated(0x67, false, false), Some('7'));
        assert_eq!(translated(0x37, true, false), None);
        assert_eq!(translated(0xe5, false, false), None); // VK_PROCESSKEY
    }
}
