//! 低级键盘钩子全局快捷键实现
//!
//! 使用 SetWindowsHookEx(WH_KEYBOARD_LL) 替代 RegisterHotKey，
//! 可穿透全屏游戏等独占全屏应用。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use parking_lot::{Mutex, RwLock};

/// 快捷键按键状态
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyState {
    Pressed,
    Released,
}

/// 快捷键回调类型：接收 AppHandle 和 按键状态
pub type ShortcutCallback = Arc<dyn Fn(&tauri::AppHandle, KeyState) + Send + Sync>;

// 修饰键位掩码
const MOD_CTRL: u8 = 0x01;
const MOD_ALT: u8 = 0x02;
const MOD_SHIFT: u8 = 0x04;
const MOD_WIN: u8 = 0x08;

/// (修饰键位掩码, 虚拟键码)
type ShortcutKey = (u8, u32);

struct Entry {
    callback: ShortcutCallback,
}

/// 已注册的快捷键表
static REGISTRY: LazyLock<RwLock<HashMap<ShortcutKey, Entry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 当前处于按下状态的快捷键（用于 Released 检测）
static ACTIVE: LazyLock<Mutex<HashSet<ShortcutKey>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// 全局 AppHandle
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// 钩子线程 ID（用于发送 WM_QUIT 停止）
#[cfg(target_os = "windows")]
static HOOK_THREAD_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 快捷键是否启用
static ENABLED: AtomicBool = AtomicBool::new(true);


// ── 公开 API ──────────────────────────────────────────────────────────

/// 启动低级键盘钩子线程（仅调用一次）
pub fn start(app: tauri::AppHandle) {
    let _ = APP_HANDLE.set(app);
    std::thread::Builder::new()
        .name("ll-shortcut-hook".into())
        .spawn(|| {
            #[cfg(target_os = "windows")]
            run_hook_loop();
        })
        .expect("failed to spawn ll-shortcut-hook thread");
}

/// 停止钩子线程
#[allow(dead_code)]
pub fn stop() {
    #[cfg(target_os = "windows")]
    {
        let tid = HOOK_THREAD_ID.swap(0, Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
                // WM_QUIT = 0x0012
                let _ = PostThreadMessageW(tid, 0x0012, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 注册快捷键，返回是否成功
pub fn register(shortcut_str: &str, callback: ShortcutCallback) -> bool {
    let key = match parse_shortcut_key(shortcut_str) {
        Some(k) => k,
        None => return false,
    };
    REGISTRY.write().insert(key, Entry { callback });
    tracing::debug!("低级钩子: 注册快捷键 {} -> ({:#04x}, {:#04x})", shortcut_str, key.0, key.1);
    true
}

/// 注销快捷键
pub fn unregister(shortcut_str: &str) {
    if let Some(key) = parse_shortcut_key(shortcut_str) {
        REGISTRY.write().remove(&key);
        ACTIVE.lock().remove(&key);
        tracing::debug!("低级钩子: 注销快捷键 {} -> ({:#04x}, {:#04x})", shortcut_str, key.0, key.1);
    }
}

/// 临时禁用所有快捷键
pub fn disable_all() {
    ENABLED.store(false, Ordering::SeqCst);
    ACTIVE.lock().clear();
}

/// 重新启用所有快捷键
pub fn enable_all() {
    ENABLED.store(true, Ordering::SeqCst);
}

#[allow(dead_code)]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

// ── 快捷键字符串 → (modifiers, vk) 解析 ─────────────────────────────

fn parse_shortcut_key(shortcut_str: &str) -> Option<ShortcutKey> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut mods: u8 = 0;
    let mut vk: Option<u32> = None;

    for part in &parts {
        let upper = part.to_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => mods |= MOD_CTRL,
            "ALT" => mods |= MOD_ALT,
            "SHIFT" => mods |= MOD_SHIFT,
            "WIN" | "SUPER" | "META" | "CMD" => mods |= MOD_WIN,
            _ => vk = parse_vk(&upper),
        }
    }

    vk.map(|k| (mods, k))
}

fn parse_vk(key: &str) -> Option<u32> {
    // 单个字母 A-Z
    if key.len() == 1 {
        let c = key.as_bytes()[0];
        if c.is_ascii_uppercase() {
            return Some(c as u32); // VK_A=0x41 .. VK_Z=0x5A
        }
        if c.is_ascii_digit() {
            return Some(c as u32); // VK_0=0x30 .. VK_9=0x39
        }
    }

    // F1-F24
    if key.starts_with('F') && key.len() <= 3 {
        if let Ok(n) = key[1..].parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x6F + n); // VK_F1=0x70
            }
        }
    }

    // Numpad0-Numpad9
    if let Some(rest) = key.strip_prefix("NUMPAD") {
        if let Ok(n) = rest.parse::<u32>() {
            if n <= 9 {
                return Some(0x60 + n); // VK_NUMPAD0=0x60
            }
        }
    }

    match key {
        "SPACE" => Some(0x20),
        "TAB" => Some(0x09),
        "ENTER" | "RETURN" => Some(0x0D),
        "BACKSPACE" => Some(0x08),
        "DELETE" | "DEL" => Some(0x2E),
        "ESCAPE" | "ESC" => Some(0x1B),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "UP" | "ARROWUP" => Some(0x26),
        "DOWN" | "ARROWDOWN" => Some(0x28),
        "LEFT" | "ARROWLEFT" => Some(0x25),
        "RIGHT" | "ARROWRIGHT" => Some(0x27),
        "INSERT" | "INS" => Some(0x2D),
        "`" | "BACKQUOTE" => Some(0xC0),
        "-" | "MINUS" => Some(0xBD),
        "=" | "EQUAL" => Some(0xBB),
        "[" | "BRACKETLEFT" => Some(0xDB),
        "]" | "BRACKETRIGHT" => Some(0xDD),
        "\\" | "BACKSLASH" => Some(0xDC),
        ";" | "SEMICOLON" => Some(0xBA),
        "'" | "QUOTE" => Some(0xDE),
        "," | "COMMA" => Some(0xBC),
        "." | "PERIOD" => Some(0xBE),
        "/" | "SLASH" => Some(0xBF),
        "PRINTSCREEN" => Some(0x2C),
        "SCROLLLOCK" => Some(0x91),
        "PAUSE" => Some(0x13),
        _ => None,
    }
}

// ── Windows 低级键盘钩子实现 ─────────────────────────────────────────

#[cfg(target_os = "windows")]
fn run_hook_loop() {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        let tid = GetCurrentThreadId();
        HOOK_THREAD_ID.store(tid, Ordering::SeqCst);

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0);

        if let Err(e) = &hook {
            tracing::error!("低级键盘钩子安装失败: {:?}", e);
            return;
        }
        let hook = hook.unwrap();

        tracing::info!("低级键盘钩子已安装（可穿透全屏应用）");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        ACTIVE.lock().clear();
        tracing::info!("低级键盘钩子已卸载");
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn hook_proc(
    code: i32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WM_KEYDOWN_U: usize = 0x0100;
    const WM_KEYUP_U: usize = 0x0101;
    const WM_SYSKEYDOWN_U: usize = 0x0104;
    const WM_SYSKEYUP_U: usize = 0x0105;
    // LLKHF_INJECTED
    const INJECTED_FLAG: u32 = 0x10;

    if code >= 0 && ENABLED.load(Ordering::Relaxed) {
        let kbd = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = kbd.vkCode;
        let w = wparam.0 as usize;

        // 跳过注入的按键（如 SendInput 模拟粘贴），避免假触发
        if kbd.flags.0 & INJECTED_FLAG != 0 {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        // 跳过修饰键本身
        if !is_modifier_vk(vk) {
            let is_press = w == WM_KEYDOWN_U || w == WM_SYSKEYDOWN_U;
            let is_release = w == WM_KEYUP_U || w == WM_SYSKEYUP_U;

            if is_press {
                let mods = get_current_modifiers();
                let key = (mods, vk);

                let callback = REGISTRY.read().get(&key).map(|e| e.callback.clone());
                if let Some(cb) = callback {
                    ACTIVE.lock().insert(key);
                    // Win/Alt 作为修饰键时，注入一个无害按键打断系统的
                    // “修饰键单独按下”检测，防止开始菜单/菜单栏被激活
                    if mods & (MOD_WIN | MOD_ALT) != 0 {
                        inject_dummy_key();
                    }
                    if let Some(app) = APP_HANDLE.get() {
                        let app = app.clone();
                        std::thread::spawn(move || {
                            cb(&app, KeyState::Pressed);
                        });
                    }
                    // 吞掉按键，不传递给目标应用
                    return LRESULT(1);
                }
            } else if is_release {
                let mut to_release = Vec::new();
                {
                    let mut active = ACTIVE.lock();
                    active.retain(|k| {
                        if k.1 == vk {
                            to_release.push(*k);
                            false
                        } else {
                            true
                        }
                    });
                }
                if !to_release.is_empty() {
                    for key in to_release {
                        let callback = REGISTRY.read().get(&key).map(|e| e.callback.clone());
                        if let Some(cb) = callback {
                            if let Some(app) = APP_HANDLE.get() {
                                let app = app.clone();
                                std::thread::spawn(move || {
                                    cb(&app, KeyState::Released);
                                });
                            }
                        }
                    }
                    // 吞掉释放按键
                    return LRESULT(1);
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 注入一个无害按键（VK_NONAME = 0xFC），打断 Windows 对 Win/Alt 键
/// "单独按下"的检测，防止触发开始菜单或菜单栏激活。
/// 该按键带有 INJECTED 标志，会被本钩子的 INJECTED_FLAG 检测跳过。
#[cfg(target_os = "windows")]
fn inject_dummy_key() {
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xFC), // VK_NONAME
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xFC),
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "windows")]
fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 |       // VK_SHIFT, VK_CONTROL, VK_MENU
        0xA0 | 0xA1 |               // VK_LSHIFT, VK_RSHIFT
        0xA2 | 0xA3 |               // VK_LCONTROL, VK_RCONTROL
        0xA4 | 0xA5 |               // VK_LMENU, VK_RMENU
        0x5B | 0x5C                  // VK_LWIN, VK_RWIN
    )
}

#[cfg(target_os = "windows")]
fn get_current_modifiers() -> u8 {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    let mut mods: u8 = 0;
    unsafe {
        if GetAsyncKeyState(0x11) < 0 { mods |= MOD_CTRL; }   // VK_CONTROL
        if GetAsyncKeyState(0x12) < 0 { mods |= MOD_ALT; }    // VK_MENU
        if GetAsyncKeyState(0x10) < 0 { mods |= MOD_SHIFT; }  // VK_SHIFT
        if GetAsyncKeyState(0x5B) < 0 || GetAsyncKeyState(0x5C) < 0 {
            mods |= MOD_WIN;
        }
    }
    mods
}
