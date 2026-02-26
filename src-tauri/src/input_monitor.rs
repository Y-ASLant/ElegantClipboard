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
use tracing::{error, info, warn};

#[cfg(windows)]
use std::cell::RefCell;
#[cfg(windows)]
use windows::Win32::Foundation::*;
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::*;

// 自定义线程消息，用于跨线程控制键盘钩子生命周期（WM_USER = 0x0400）
#[cfg(windows)]
const MSG_INSTALL_KB_HOOK: u32 = 0x0401;
#[cfg(windows)]
const MSG_UNINSTALL_KB_HOOK: u32 = 0x0402;

/// 主窗口引用，用于点击检测
static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);

/// 主窗口自身的 HWND（Windows 下初始化时填入，用于 WinEventHook 回调中过滤本窗口）
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

/// 窗口是否可见（监控是否激活）
static MOUSE_MONITORING_ENABLED: AtomicBool = AtomicBool::new(false);

/// 窗口是否固定（固定时不因外部点击隐藏）
static WINDOW_PINNED: AtomicBool = AtomicBool::new(false);

/// 搜索框聚焦前保存的前台窗口句柄（用于搜索框失焦后还原）
static PREV_FOREGROUND_HWND: AtomicIsize = AtomicIsize::new(0);

/// 监控线程是否正在运行
static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

/// 缓存的光标坐标（由鼠标钩子更新）
static CURSOR_X: AtomicI64 = AtomicI64::new(0);
static CURSOR_Y: AtomicI64 = AtomicI64::new(0);

/// 钩子线程 ID，用于 PostThreadMessage
#[cfg(windows)]
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

// 低级钩子（LL hook）必须由安装它的线程负责卸载，使用 thread_local 存储句柄
#[cfg(windows)]
thread_local! {
    static TL_MOUSE_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
    static TL_KEYBOARD_HOOK: RefCell<Option<HHOOK>> = const { RefCell::new(None) };
}

/// 初始化，传入主窗口引用
pub fn init(window: WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    *MAIN_WINDOW.lock() = Some(window);
}

/// 启动全局输入监控线程
pub fn start_monitoring() {
    if MONITOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        warn!("输入监控已在运行");
        return;
    }

    thread::spawn(|| {
        #[cfg(windows)]
        run_hook_thread();

        #[cfg(not(windows))]
        warn!("当前平台不支持输入监控");

        MONITOR_RUNNING.store(false, Ordering::SeqCst);
        #[cfg(windows)]
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
    });

    info!("输入监控已启动");
}

/// 停止输入监控（向钩子线程发送 WM_QUIT）
#[allow(dead_code)]
pub fn stop_monitoring() {
    MONITOR_RUNNING.store(false, Ordering::SeqCst);
    #[cfg(windows)]
    {
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 启用监控并安装键盘钩子（窗口显示时调用）
pub fn enable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, MSG_INSTALL_KB_HOOK, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 禁用监控并卸载键盘钩子（窗口隐藏时调用）
pub fn disable_mouse_monitoring() {
    MOUSE_MONITORING_ENABLED.store(false, Ordering::Relaxed);
    #[cfg(windows)]
    {
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, MSG_UNINSTALL_KB_HOOK, WPARAM(0), LPARAM(0));
            }
        }
    }
}

#[allow(dead_code)]
pub fn is_mouse_monitoring_enabled() -> bool {
    MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed)
}

/// 设置窗口固定状态（固定时不因外部点击隐藏）
pub fn set_window_pinned(pinned: bool) {
    WINDOW_PINNED.store(pinned, Ordering::Relaxed);
}

pub fn is_window_pinned() -> bool {
    WINDOW_PINNED.load(Ordering::Relaxed)
}

/// 保存当前前台窗口句柄（搜索框聚焦前 / 窗口显示前调用）
#[cfg(windows)]
pub fn save_current_focus() {
    let hwnd = unsafe { GetForegroundWindow() };
    let val = hwnd.0 as isize;
    // 过滤本窗口
    let main_raw = MAIN_HWND.load(Ordering::Relaxed);
    if main_raw != 0 && val == main_raw {
        return;
    }
    PREV_FOREGROUND_HWND.store(val, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub fn save_current_focus() {}

/// 临时启用窗口焦点（供搜索框输入使用）。
/// 先保存当前前台窗口，再 set_focusable(true) + set_focus()。
pub fn focus_clipboard_window(window: &tauri::WebviewWindow) {
    save_current_focus();
    let _ = window.set_focusable(true);
    let _ = window.set_focus();
}

/// 恢复非聚焦模式并还原之前的前台窗口。
/// 搜索框 blur 时调用，让目标应用重新获得焦点。
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
    let _ = window.set_focusable(false);
}

/// 获取当前光标坐标（供定位模块使用）
pub fn get_cursor_position() -> (f64, f64) {
    let x = CURSOR_X.load(Ordering::Relaxed) as f64;
    let y = CURSOR_Y.load(Ordering::Relaxed) as f64;
    (x, y)
}

// ─── Windows 钩子实现 ─────────────────────────────────────────────────────────

/// 钩子线程主函数：安装 WH_MOUSE_LL 和 WinEventHook(FOREGROUND)，运行消息循环，
/// 并通过自定义消息动态管理 WH_KEYBOARD_LL 生命周期。
#[cfg(windows)]
fn run_hook_thread() {
    // 安装鼠标钩子（始终保持，用于点击外部检测）
    let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) };
    match mouse_hook {
        Ok(hook) => {
            TL_MOUSE_HOOK.with(|h| *h.borrow_mut() = Some(hook));
            info!("WH_MOUSE_LL 钩子已安装");
        }
        Err(e) => {
            error!("WH_MOUSE_LL 钩子安装失败: {:?}", e);
            return;
        }
    }

    // 安装前台窗口变化钩子，持续追踪最近活跃的非本窗口 HWND
    // WINEVENT_OUTOFCONTEXT(0) = 进程外钩子，无需注入 DLL，回调在本线程消息循环中触发
    let focus_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_proc),
            0, // 监听所有进程
            0, // 监听所有线程
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if focus_hook.0.is_null() {
        warn!("WinEventHook(FOREGROUND) 安装失败，固定模式焦点还原可能不准确");
    } else {
        info!("WinEventHook(FOREGROUND) 已安装");
    }

    HOOK_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);

    // 消息循环：GetMessageW 阻塞等待消息，收到 WM_QUIT 时退出
    let mut msg = MSG::default();
    loop {
        let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        // ret == 0 → WM_QUIT，ret.0 < 0 → 错误
        if ret.0 <= 0 {
            break;
        }

        match msg.message {
            MSG_INSTALL_KB_HOOK => {
                // 仅在尚未安装时安装键盘钩子
                let already = TL_KEYBOARD_HOOK.with(|h| h.borrow().is_some());
                if !already {
                    let kb_hook = unsafe {
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0)
                    };
                    match kb_hook {
                        Ok(hook) => TL_KEYBOARD_HOOK.with(|h| *h.borrow_mut() = Some(hook)),
                        Err(e) => error!("WH_KEYBOARD_LL 钩子安装失败: {:?}", e),
                    }
                }
            }
            MSG_UNINSTALL_KB_HOOK => {
                // 窗口已隐藏，卸载键盘钩子
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

    // 退出时清理所有钩子
    for cleanup in [&TL_MOUSE_HOOK, &TL_KEYBOARD_HOOK] {
        cleanup.with(|h| {
            if let Some(hook) = h.borrow_mut().take() {
                unsafe { let _ = UnhookWindowsHookEx(hook); }
            }
        });
    }
    if !focus_hook.0.is_null() {
        unsafe { let _ = UnhookWinEvent(focus_hook); }
    }

    info!("输入监控线程已退出");
}

/// WH_MOUSE_LL 回调：追踪光标位置，检测窗口外点击
#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        match wparam.0 as u32 {
            v if v == WM_MOUSEMOVE => {
                if MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) {
                    if let Some(info) = (lparam.0 as *const MSLLHOOKSTRUCT).as_ref() {
                        CURSOR_X.store(info.pt.x as i64, Ordering::Relaxed);
                        CURSOR_Y.store(info.pt.y as i64, Ordering::Relaxed);
                    }
                }
            }
            v if v == WM_LBUTTONDOWN || v == WM_RBUTTONDOWN => {
                handle_click_outside();
            }
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// WH_KEYBOARD_LL 回调：检测 ESC 键以隐藏窗口。
/// 此钩子仅在窗口可见时安装。
#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_KEYDOWN {
        if let Some(info) = (lparam.0 as *const KBDLLHOOKSTRUCT).as_ref() {
            if info.vkCode == u32::from(VK_ESCAPE.0) {
                handle_escape_key();
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// WinEvent 回调：前台窗口变化通知（保留 hook 以备后续扩展）。
#[cfg(windows)]
unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
}

// ─── 事件处理 ─────────────────────────────────────────────────────────────────

/// 检查光标是否在窗口边界外
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
    let win_width = size.width as f64;
    let win_height = size.height as f64;

    cursor_x < win_x
        || cursor_x > win_x + win_width
        || cursor_y < win_y
        || cursor_y > win_y + win_height
}

/// 检查监控是否处于可响应状态（未禁用且未固定）
fn is_monitoring_active() -> bool {
    MOUSE_MONITORING_ENABLED.load(Ordering::Relaxed) && !WINDOW_PINNED.load(Ordering::Relaxed)
}

/// 处理 ESC 按键：向前端发送事件，由前端决定关闭弹窗或隐藏窗口
fn handle_escape_key() {
    if !is_monitoring_active() {
        return;
    }
    if let Some(window) = MAIN_WINDOW.lock().as_ref() {
        if window.is_visible().unwrap_or(false) {
            let _ = window.emit("escape-pressed", ());
        }
    }
}

/// 处理外部点击：若点击在窗口边界外则隐藏窗口
fn handle_click_outside() {
    if !is_monitoring_active() {
        return;
    }
    if let Some(window) = MAIN_WINDOW.lock().as_ref() {
        if window.is_visible().unwrap_or(false) && is_mouse_outside_window(window) {
            crate::save_window_size_if_enabled(window.app_handle(), window);
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
            // disable_mouse_monitoring 会向本线程投递 MSG_UNINSTALL_KB_HOOK，
            // 该消息将在当前钩子回调返回后的下一次消息循环中处理
            disable_mouse_monitoring();
            crate::commands::hide_image_preview_window(window.app_handle());
            let _ = window.emit("window-hidden", ());
        }
    }
}
