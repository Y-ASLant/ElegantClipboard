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
    GetAsyncKeyState, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_UP,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::*;

#[cfg(windows)]
const MSG_INSTALL_KB_HOOK: u32 = 0x0401;
#[cfg(windows)]
const MSG_UNINSTALL_KB_HOOK: u32 = 0x0402;

static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

static TRANSLATE_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

static TRANSLATE_HWND: AtomicIsize = AtomicIsize::new(0);

static MOUSE_MONITORING_ENABLED: AtomicBool = AtomicBool::new(false);

static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

static TRANSLATE_WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

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
    unsafe {
        CallWindowProcW(
            #[allow(clippy::missing_transmute_annotations)]
            Some(std::mem::transmute(original)),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}

pub fn init(window: WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        let raw_hwnd = HWND(hwnd.0.cast());
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
    if is_translate_window_visible() {
        return;
    }
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

pub fn setup_translate_window(window: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        TRANSLATE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    *TRANSLATE_WINDOW.lock() = Some(window.clone());
    TRANSLATE_WINDOW_PINNED.store(false, Ordering::Relaxed);

    let window = window.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            cleanup_translate_window();
        }
    });
}

pub fn translate_window_shown() {
    TRANSLATE_WINDOW_PINNED.store(false, Ordering::Relaxed);
    enable_mouse_monitoring();
}

pub fn cleanup_translate_window() {
    TRANSLATE_HWND.store(0, Ordering::Relaxed);
    *TRANSLATE_WINDOW.lock() = None;
    TRANSLATE_WINDOW_PINNED.store(false, Ordering::Relaxed);
    if !is_main_window_visible() {
        disable_mouse_monitoring();
    }
}

pub fn set_translate_window_pinned(pinned: bool) {
    TRANSLATE_WINDOW_PINNED.store(pinned, Ordering::Relaxed);
}

pub fn is_translate_window_pinned() -> bool {
    TRANSLATE_WINDOW_PINNED.load(Ordering::Relaxed)
}

fn is_main_window_visible() -> bool {
    MAIN_WINDOW
        .lock()
        .as_ref()
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

fn is_translate_window_visible() -> bool {
    TRANSLATE_WINDOW
        .lock()
        .as_ref()
        .is_some_and(|window| window.is_visible().unwrap_or(false))
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


/// 临时启用窗口焦点（供搜索框输入使用）。
pub fn focus_clipboard_window(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    if let Err(err) = crate::main_thread::run_on_ui_thread(&app, {
        let app = app.clone();
        move || {
            if let Some(window) = app.get_webview_window("main") {
                save_current_focus();
                let _ = window.set_focusable(true);
                let _ = window.set_focus();
            }
        }
    }) {
        warn!("focus_clipboard_window dispatch failed: {err}");
    }
}

/// 恢复非聚焦模式并还原之前的前台窗口（搜索框 blur 时调用）。
#[cfg(windows)]
pub fn restore_last_focus(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    if let Err(err) = crate::main_thread::run_on_ui_thread(&app, {
        let app = app.clone();
        move || {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focusable(false);
            }
            let raw = PREV_FOREGROUND_HWND.load(Ordering::Relaxed);
            if raw != 0 {
                let hwnd = HWND(raw as *mut _);
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }
    }) {
        warn!("restore_last_focus dispatch failed: {err}");
    }
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
        let kb_hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };
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
        let ret = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
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
                        unsafe {
                            let _ = UnhookWindowsHookEx(hook);
                        }
                    }
                });
            }
            _ => unsafe {
                let _ = TranslateMessage(&raw const msg);
                let _ = DispatchMessageW(&raw const msg);
            },
        }
    }

    for cleanup in [&TL_MOUSE_HOOK, &TL_KEYBOARD_HOOK] {
        cleanup.with(|h| {
            if let Some(hook) = h.borrow_mut().take() {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
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
                    && let Some(info) = unsafe { (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() }
                {
                    CURSOR_X.store(i64::from(info.pt.x), Ordering::Relaxed);
                    CURSOR_Y.store(i64::from(info.pt.y), Ordering::Relaxed);
                }
            }
            v if v == WM_LBUTTONDOWN || v == WM_RBUTTONDOWN => {
                // 用点击坐标更新光标位置，确保边界检查精确
                if let Some(info) = unsafe { (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() } {
                    CURSOR_X.store(i64::from(info.pt.x), Ordering::Relaxed);
                    CURSOR_Y.store(i64::from(info.pt.y), Ordering::Relaxed);
                }
                handle_click_outside();
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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
                        v if v == u32::from(VK_UP.0) => Some("ArrowUp"),
                        v if v == u32::from(VK_DOWN.0) => Some("ArrowDown"),
                        v if v == u32::from(VK_LEFT.0) => Some("ArrowLeft"),
                        v if v == u32::from(VK_RIGHT.0) => Some("ArrowRight"),
                        v if v == u32::from(VK_RETURN.0) => Some("Enter"),
                        v if v == u32::from(VK_DELETE.0) => Some("Delete"),
                        _ => None,
                    };
                    if let Some(key) = nav_key {
                        if is_keydown {
                            let shift = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) < 0 };
                            handle_nav_key(key, shift);
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
fn handle_nav_key(key: &'static str, shift: bool) {
    dispatch_on_main_thread(move |_app, window| {
        if window.is_visible().unwrap_or(false) {
            let _ = window.emit(
                "keyboard-nav",
                serde_json::json!({
                    "key": key,
                    "shift": shift,
                }),
            );
        }
    });
}

#[cfg(windows)]
fn is_mouse_outside_translate_window() -> bool {
    is_mouse_outside_hwnd(TRANSLATE_HWND.load(Ordering::Relaxed))
}

#[cfg(windows)]
fn is_mouse_outside_hwnd(raw: isize) -> bool {
    if raw == 0 {
        return false;
    }

    let cx = CURSOR_X.load(Ordering::Relaxed) as i32;
    let cy = CURSOR_Y.load(Ordering::Relaxed) as i32;

    let hwnd = HWND(raw as *mut _);
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &raw mut rect) }.is_err() {
        return false;
    }

    cx < rect.left || cx > rect.right || cy < rect.top || cy > rect.bottom
}

#[cfg(windows)]
fn is_mouse_outside_window(_window: &WebviewWindow) -> bool {
    let outside = is_mouse_outside_hwnd(MAIN_HWND.load(Ordering::Relaxed));
    if outside {
        debug!("点击检测: cursor outside main window");
    }
    outside
}

fn is_monitoring_active() -> bool {
    MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed)
}

/// 低级钩子回调不在主线程，不能直接操作 WebView 窗口；派发到主线程执行。
fn dispatch_on_main_thread(f: impl FnOnce(&tauri::AppHandle, &WebviewWindow) + Send + 'static) {
    let Some(app) = MAIN_WINDOW
        .lock()
        .as_ref()
        .map(|window| window.app_handle().clone())
        .or_else(|| {
            TRANSLATE_WINDOW
                .lock()
                .as_ref()
                .map(|window| window.app_handle().clone())
        })
    else {
        return;
    };
    if let Err(err) = crate::main_thread::run_on_ui_thread(&app, {
        let app = app.clone();
        move || {
            if let Some(window) = app.get_webview_window("main") {
                f(&app, &window);
            }
        }
    }) {
        warn!("Failed to dispatch input monitor action to main thread: {err}");
    }
}

fn dispatch_translate_action(f: impl FnOnce(&tauri::AppHandle, &WebviewWindow) + Send + 'static) {
    let Some(app) = TRANSLATE_WINDOW
        .lock()
        .as_ref()
        .map(|window| window.app_handle().clone())
        .or_else(|| {
            MAIN_WINDOW
                .lock()
                .as_ref()
                .map(|window| window.app_handle().clone())
        })
    else {
        return;
    };
    if let Err(err) = crate::main_thread::run_on_ui_thread(&app, {
        let app = app.clone();
        move || {
            if let Some(window) = app.get_webview_window("translate-result") {
                f(&app, &window);
            }
        }
    }) {
        warn!("Failed to dispatch translate window action to main thread: {err}");
    }
}

fn handle_escape_key() {
    if !is_monitoring_active() {
        return;
    }
    dispatch_on_main_thread(|_app, window| {
        if window.is_visible().unwrap_or(false) {
            let _ = window.emit("escape-pressed", ());
        }
    });
}

fn handle_click_outside() {
    if !MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) {
        trace!("handle_click_outside: skipped (monitoring inactive)");
        return;
    }

    if !TRANSLATE_WINDOW_PINNED.load(Ordering::Relaxed) {
        dispatch_translate_action(|_app, window| {
            if window.is_visible().unwrap_or(false) && is_mouse_outside_translate_window() {
                info!("handle_click_outside: translate window visible and click outside, closing");
                let _ = window.close();
            }
        });
    }

    if WINDOW_PINNED.load(Ordering::Relaxed) {
        return;
    }

    dispatch_on_main_thread(|app, window| {
        if window.is_visible().unwrap_or(false) && is_mouse_outside_window(window) {
            info!("handle_click_outside: window visible and click outside, hiding");
            crate::commands::window::hide_main_window_inner(app, window);
        }
    });
}
