//! Low-level keyboard hook that forwards navigation keys to the history window
//! while it is shown without activation, so the paste target keeps focus.
pub use crate::key_intercept_types::InterceptedKey;
use crate::key_intercept_types::{CONTROL_LETTERS, NAVIGATION_KEYS};
use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use std::{
    cell::RefCell,
    collections::HashSet,
    sync::{Mutex, mpsc},
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_CHAR, MapVirtualKeyW, SendInput,
            VIRTUAL_KEY, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT,
            VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT, VK_UP,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HC_ACTION,
            IsWindowVisible, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, PM_NOREMOVE,
            PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
            WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
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
    swallowed: HashSet<u32>,
}

thread_local! {
    static HOOK_TARGET: RefCell<Option<HookTarget>> = const { RefCell::new(None) };
}

/// Text keys swallowed before the window took focus, replayed in order.
static PENDING_TEXT: Mutex<Vec<(u16, u16, bool)>> = Mutex::new(Vec::new());

fn key_down(key: VIRTUAL_KEY) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(key.0)) }) < 0
}

fn foreground_is_ours() -> bool {
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut process_id)) };
    process_id == unsafe { GetCurrentProcessId() }
}

fn classify(vk: u32) -> Option<InterceptedKey> {
    if key_down(VK_MENU) || key_down(VK_LWIN) || key_down(VK_RWIN) {
        return None;
    }
    let control = key_down(VK_CONTROL);
    let shift = key_down(VK_SHIFT);
    if let Some(index) = NAVIGATION_VKS.iter().position(|key| u32::from(key.0) == vk) {
        return Some(InterceptedKey::Shortcut {
            key: NAVIGATION_KEYS[index],
            control,
            shift,
        });
    }
    let character = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0x7fff;
    if control {
        let letter = char::from_u32(character)?.to_ascii_lowercase();
        return CONTROL_LETTERS
            .into_iter()
            .find(|name| name.starts_with(letter))
            .map(|key| InterceptedKey::Shortcut {
                key,
                control,
                shift,
            });
    }
    (character >= 0x20).then_some(InterceptedKey::Text)
}

unsafe extern "system" fn keyboard_hook(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    let mut swallow = false;
    if code == HC_ACTION as i32 {
        let info = unsafe { *(data.0 as *const KBDLLHOOKSTRUCT) };
        let pressed = matches!(message.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if info.flags.0 & LLKHF_INJECTED.0 == 0 {
            HOOK_TARGET.with(|target| {
                let mut target = target.borrow_mut();
                let Some(target) = target.as_mut() else {
                    return;
                };
                if !pressed {
                    swallow = target.swallowed.remove(&info.vkCode);
                    return;
                }
                if !unsafe { IsWindowVisible(HWND(target.hwnd as *mut _)) }.as_bool()
                    || foreground_is_ours()
                {
                    return;
                }
                // Never block the hook on the UI; classification reads async key state only.
                let Some(key) = classify(info.vkCode) else {
                    return;
                };
                if key == InterceptedKey::Text {
                    PENDING_TEXT
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push((
                            info.vkCode as u16,
                            info.scanCode as u16,
                            info.flags.0 & LLKHF_EXTENDED.0 != 0,
                        ));
                }
                target.swallowed.insert(info.vkCode);
                let _ = target.sender.try_send(key);
                swallow = true;
            });
        }
    }
    if swallow {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, message, data) }
    }
}

/// Replay swallowed text keys after the history window has taken focus.
pub fn replay_pending_text() -> Result<()> {
    let keys = std::mem::take(&mut *PENDING_TEXT.lock().unwrap_or_else(|e| e.into_inner()));
    let inputs: Vec<INPUT> = keys
        .into_iter()
        .flat_map(|(vk, scan, extended)| {
            let flags = if extended {
                KEYEVENTF_EXTENDEDKEY
            } else {
                KEYBD_EVENT_FLAGS(0)
            };
            [flags, flags | KEYEVENTF_KEYUP].map(|key_flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scan,
                        dwFlags: key_flags,
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
                        swallowed: HashSet::new(),
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
