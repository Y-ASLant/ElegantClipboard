//! 全局输入监控（点击外部隐藏窗口）
//!
//! - WH_MOUSE_LL：始终保持，用于检测窗口外点击。
//! - WH_KEYBOARD_LL：**仅窗口可见时安装**，用于 ESC 键检测。
//!
//! # 为何不用 rdev？
//! `rdev::listen` 会在整个 App 生命周期内同时安装 WH_MOUSE_LL 和
//! WH_KEYBOARD_LL。WH_KEYBOARD_LL 使 Windows 在每次按键送达前台应用前
//! 先经过本进程回调，Firefox/Gecko 内核（如 Zen Browser）对此极其敏感，
//! 哪怕微小延迟也会触发漏斗光标。
//!
//! 将 WH_KEYBOARD_LL 改为仅在窗口可见时安装，用户在其他应用打字时
//! 完全不受影响。

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU32, Ordering};
use std::thread;
use tauri::{Emitter, Manager, WebviewWindow};
use tracing::{debug, error, info, trace, warn};

#[cfg(windows)]
use std::cell::RefCell;
#[cfg(windows)]
use windows::Win32::Foundation::*;
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT,
    VK_UP,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::*;

#[cfg(windows)]
const MSG_INSTALL_KB_HOOK: u32 = 0x0401;
#[cfg(windows)]
const MSG_UNINSTALL_KB_HOOK: u32 = 0x0402;

static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

static MOUSE_MONITORING_ENABLED: AtomicBool = AtomicBool::new(false);

static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

static PREV_FOREGROUND_HWND: AtomicIsize = AtomicIsize::new(0);

static KEYBOARD_NAV_ENABLED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static KEYBOARD_HOOK_DESIRED: AtomicBool = AtomicBool::new(false);

static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

static CURSOR_X: AtomicI64 = AtomicI64::new(0);
static CURSOR_Y: AtomicI64 = AtomicI64::new(0);

#[cfg(windows)]
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

#[cfg(windows)]
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);

// 低级钩子（LL hook）必须由安装它的线程负责卸载，使用 thread_local 存储句柄
#[cfg(windows)]
thread_local! {
    static TL_MOUSE_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
    static TL_KEYBOARD_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
}

#[cfg(windows)]
unsafe extern "system" fn wndproc_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        if ex_style & WS_EX_NOACTIVATE.0 != 0 {
            return LRESULT(3); // MA_NOACTIVATE
        }
    }

    let original = ORIGINAL_WNDPROC.load(Ordering::Relaxed);
    if original == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    unsafe { CallWindowProcW(Some(std::mem::transmute(original)), hwnd, msg, wparam, lparam) }
}

pub fn init(window: WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        let raw_hwnd = HWND(hwnd.0 as *mut _);
        let original = unsafe {
            SetLastError(WIN32_ERROR(0));
            SetWindowLongPtrW(
                raw_hwnd,
                GWLP_WNDPROC,
                wndproc_subclass as *const () as usize as isize,
            )
        };
        let last_error = unsafe { GetLastError() };
        if original == 0 && last_error != WIN32_ERROR(0) {
            warn!("Failed to subclass main window WndProc: {:?}", last_error);
        } else {
            ORIGINAL_WNDPROC.store(original, Ordering::Relaxed);
        }
    }
    *MAIN_WINDOW.lock() = Some(window);
}

pub fn start_monitoring() {
    if MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        warn!("Input monitor already running");
        return;
    }

    thread::spawn(|| {
        #[cfg(windows)]
        run_hook_thread();

        #[cfg(not(windows))]
        warn!("Input monitoring not supported on this platform");

        MONITOR_RUNNING.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
    });

    info!("Input monitor started");
}

pub fn enable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        KEYBOARD_HOOK_DESIRED.store(true, Ordering::SeqCst);
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                if PostThreadMessageW(tid, MSG_INSTALL_KB_HOOK, WPARAM(0), LPARAM(0)).is_err() {
                    warn!("Failed to post keyboard hook install message to hook thread");
                }
            }
        }
    }
}

pub fn disable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(false, Ordering::Relaxed);
    #[cfg(windows)]
    {
        KEYBOARD_HOOK_DESIRED.store(false, Ordering::SeqCst);
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                if PostThreadMessageW(tid, MSG_UNINSTALL_KB_HOOK, WPARAM(0), LPARAM(0)).is_err() {
                    warn!("Failed to post keyboard hook uninstall message to hook thread");
                }
            }
        }
    }
}

pub fn set_window_pinned(pinned: bool) {
    WINDOW_PINNED.store(pinned, Ordering::Relaxed);
}

pub fn is_window_pinned() -> bool {
    WINDOW_PINNED.load(Ordering::Relaxed)
}

pub fn set_keyboard_nav_enabled(enabled: bool) {
    KEYBOARD_NAV_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn get_prev_foreground_hwnd() -> isize {
    PREV_FOREGROUND_HWND.load(Ordering::Relaxed)
}

#[cfg(windows)]
pub fn save_current_focus() {
    let hwnd = unsafe { GetForegroundWindow() };
    let val = hwnd.0 as isize;
    let main_raw = MAIN_HWND.load(Ordering::Relaxed);
    if main_raw != 0 && val == main_raw {
        return;
    }
    PREV_FOREGROUND_HWND.store(val, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub fn save_current_focus() {}

/// 临时启用窗口焦点（供搜索框输入使用）。
pub fn focus_clipboard_window(window: &tauri::WebviewWindow) {
    save_current_focus();
    let _ = window.set_focusable(true);
    let _ = window.set_focus();
}

/// 恢复非聚焦模式并还原之前的前台窗口（搜索框 blur 时调用）。
#[cfg(windows)]
pub fn restore_last_focus(window: &tauri::WebviewWindow) {
    let _ = window.set_focusable(false);
    let raw = PREV_FOREGROUND_HWND.load(Ordering::Relaxed);
    if raw != 0 {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn restore_last_focus(window: &tauri::WebviewWindow) {
    // 非 Windows 平台回退逻辑
    let _ = window.set_focusable(false);
}

pub fn get_cursor_position() -> (f64, f64) {
    let x = CURSOR_X.load(Ordering::Relaxed) as f64;
    let y = CURSOR_Y.load(Ordering::Relaxed) as f64;
    (x, y)
}

#[cfg(windows)]
fn run_hook_thread() {
    unsafe {
        let _ = windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        let ctx = windows::Win32::UI::HiDpi::GetThreadDpiAwarenessContext();
        let aw = windows::Win32::UI::HiDpi::GetAwarenessFromDpiAwarenessContext(ctx);
        info!("Hook thread DPI awareness: {:?}", aw);
    }

    let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) };
    match mouse_hook {
        Ok(hook) => {
            TL_MOUSE_HOOK.with(|h| *h.borrow_mut() = Some(hook));
            info!("WH_MOUSE_LL hook installed");
        }
        Err(e) => {
            error!("WH_MOUSE_LL hook install failed: {:?}", e);
            return;
        }
    }

    HOOK_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    if KEYBOARD_HOOK_DESIRED.load(Ordering::SeqCst) {
        let kb_hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };
        match kb_hook {
            Ok(hook) => {
                TL_KEYBOARD_HOOK.with(|h| *h.borrow_mut() = Some(hook));
                info!("WH_KEYBOARD_LL hook installed");
            }
            Err(e) => error!("WH_KEYBOARD_LL hook install failed: {:?}", e),
        }
    }

    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if ret.0 <= 0 {
            break;
        }

        match msg.message {
            MSG_INSTALL_KB_HOOK => {
                let already = TL_KEYBOARD_HOOK.with(|h| h.borrow().is_some());
                if !already {
                    let kb_hook = unsafe {
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0)
                    };
                    match kb_hook {
                        Ok(hook) => TL_KEYBOARD_HOOK.with(|h| *h.borrow_mut() = Some(hook)),
                        Err(e) => error!("WH_KEYBOARD_LL hook install failed: {:?}", e),
                    }
                }
            }
            MSG_UNINSTALL_KB_HOOK => {
                TL_KEYBOARD_HOOK.with(|h| {
                    if let Some(hook) = h.borrow_mut().take() {
                        unsafe { let _ = UnhookWindowsHookEx(hook); }
                    }
                });
            }
            _ => unsafe {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            },
        }
    }

    for cleanup in [&TL_MOUSE_HOOK, &TL_KEYBOARD_HOOK] {
        cleanup.with(|h| {
            if let Some(hook) = h.borrow_mut().take() {
                unsafe { let _ = UnhookWindowsHookEx(hook); }
            }
        });
    }
    info!("Input monitor thread exited");
}

#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        match wparam.0 as u32 {
            v if v == WM_MOUSEMOVE => {
                if MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed)
                    && let Some(info) = unsafe { (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() } {
                        CURSOR_X.store(info.pt.x as i64, Ordering::Relaxed);
                        CURSOR_Y.store(info.pt.y as i64, Ordering::Relaxed);
                    }
            }
            v if v == WM_LBUTTONDOWN || v == WM_RBUTTONDOWN => {
                // 用点击坐标更新光标位置，确保边界检查精确
                if let Some(info) = unsafe { (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() } {
                    CURSOR_X.store(info.pt.x as i64, Ordering::Relaxed);
                    CURSOR_Y.store(info.pt.y as i64, Ordering::Relaxed);
                }
                handle_click_outside();
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        let is_keydown = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_keyup = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if let Some(info) = unsafe { (lparam.0 as *const KBDLLHOOKSTRUCT).as_ref() } {
            if is_keydown && info.vkCode == u32::from(VK_ESCAPE.0) {
                handle_escape_key();
            }

            // 键盘导航：捕获方向键/Enter/Delete 转发前端，避免抢焦点
            if KEYBOARD_NAV_ENABLED.load(Ordering::Relaxed) && (is_keydown || is_keyup) {
                // 若本窗口已是前台（如搜索框聚焦），让按键正常走 DOM 路径
                let main_raw = MAIN_HWND.load(Ordering::Relaxed);
                let fg = unsafe { GetForegroundWindow() };
                if main_raw != 0 && fg.0 as isize != main_raw {
                    let nav_key = match info.vkCode {
                        v if v == VK_UP.0 as u32 => Some("ArrowUp"),
                        v if v == VK_DOWN.0 as u32 => Some("ArrowDown"),
                        v if v == VK_LEFT.0 as u32 => Some("ArrowLeft"),
                        v if v == VK_RIGHT.0 as u32 => Some("ArrowRight"),
                        v if v == VK_RETURN.0 as u32 => Some("Enter"),
                        v if v == VK_DELETE.0 as u32 => Some("Delete"),
                        _ => None,
                    };
                    if let Some(key) = nav_key {
                        if is_keydown {
                            handle_nav_key(key);
                        }
                        return LRESULT(1);
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(windows)]
fn handle_nav_key(key: &str) {
    if let Some(window) = MAIN_WINDOW.lock().as_ref()
        && window.is_visible().unwrap_or(false) {
            let shift = unsafe { GetAsyncKeyState(VK_SHIFT.0 as i32) < 0 };
            let _ = window.emit("keyboard-nav", serde_json::json!({
                "key": key,
                "shift": shift,
            }));
        }
}

#[cfg(windows)]
fn is_mouse_outside_window(_window: &WebviewWindow) -> bool {
    let cx = CURSOR_X.load(Ordering::Relaxed) as i32;
    let cy = CURSOR_Y.load(Ordering::Relaxed) as i32;

    let raw = MAIN_HWND.load(Ordering::Relaxed);
    if raw == 0 {
        return false;
    }

    let hwnd = HWND(raw as *mut _);
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return false;
    }

    let outside = cx < rect.left || cx > rect.right || cy < rect.top || cy > rect.bottom;
    debug!(
        "点击检测: cursor=({},{}) rect=({},{},{},{}) → {}",
        cx, cy, rect.left, rect.top, rect.right, rect.bottom,
        if outside { "outside" } else { "inside" }
    );
    outside
}

#[cfg(not(windows))]
fn is_mouse_outside_window(window: &WebviewWindow) -> bool {
    let cursor_x = CURSOR_X.load(Ordering::Relaxed) as f64;
    let cursor_y = CURSOR_Y.load(Ordering::Relaxed) as f64;

    let position = match window.outer_position() {
        Ok(pos) => pos,
        Err(_) => return false,
    };
    let size = match window.outer_size() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let win_x = position.x as f64;
    let win_y = position.y as f64;
    cursor_x < win_x
        || cursor_x > win_x + size.width as f64
        || cursor_y < win_y
        || cursor_y > win_y + size.height as f64
}

fn is_monitoring_active() -> bool {
    MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed)
}

fn handle_escape_key() {
    if !is_monitoring_active() {
        return;
    }
    if let Some(window) = MAIN_WINDOW.lock().as_ref()
        && window.is_visible().unwrap_or(false) {
            let _ = window.emit("escape-pressed", ());
        }
}

fn handle_click_outside() {
    let mouse_enabled = MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed);
    let pinned = WINDOW_PINNED.load(Ordering::Relaxed);
    if !mouse_enabled || pinned {
        trace!(
            "handle_click_outside: 跳过 (mouse_enabled={}, pinned={})",
            mouse_enabled, pinned
        );
        return;
    }
    if let Some(window) = MAIN_WINDOW.lock().as_ref()
        && window.is_visible().unwrap_or(false) && is_mouse_outside_window(window) {
            info!("handle_click_outside: window visible and click outside, hiding");
            crate::commands::window::save_window_size_if_enabled(window.app_handle(), window);
            let _ = window.set_focusable(false);
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            disable_mouse_monitoring();
            crate::commands::hide_preview_windows(window.app_handle());
            let _ = window.emit("window-hidden", ());
        }
}
