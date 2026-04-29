//! 游戏模式：检测全屏应用时自动暂停剪贴板监控和全局快捷键。
//!
//! 使用 Windows `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` 事件驱动，
//! 仅在前台窗口切换时触发检测，空闲时零 CPU 开销。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tauri::Manager;

use crate::commands::AppState;
use std::sync::Arc;

/// 游戏模式是否启用（用户设置）
static GAME_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
/// 当前是否处于抑制状态（检测到全屏应用时为 true）
static GAME_MODE_SUPPRESSED: AtomicBool = AtomicBool::new(false);
/// 监听线程的 Windows 线程 ID（用于发送 WM_QUIT 停止消息循环）
static WATCHER_THREAD_ID: AtomicU32 = AtomicU32::new(0);
/// 全局 AppHandle 引用（供事件回调使用，应用生命周期内不变）
static GAME_MODE_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

// Windows 常量
#[cfg(target_os = "windows")]
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
#[cfg(target_os = "windows")]
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

/// 启动游戏模式检测
pub fn start(app: tauri::AppHandle) {
    if GAME_MODE_ENABLED.swap(true, Ordering::SeqCst) {
        return; // 已在运行
    }
    let _ = GAME_MODE_APP.set(app);

    std::thread::Builder::new()
        .name("game-mode-watcher".into())
        .spawn(|| {
            #[cfg(target_os = "windows")]
            run_event_loop();
        })
        .expect("failed to spawn game-mode-watcher thread");
}

/// 停止游戏模式检测
pub fn stop() {
    GAME_MODE_ENABLED.store(false, Ordering::SeqCst);

    // 向事件循环线程发送 WM_QUIT 使其退出
    #[cfg(target_os = "windows")]
    {
        let tid = WATCHER_THREAD_ID.swap(0, Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                use windows::Win32::Foundation::{LPARAM, WPARAM};
                use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
                let _ = PostThreadMessageW(tid, 0x0012 /* WM_QUIT */, WPARAM(0), LPARAM(0));
            }
        }
    }
}

// ── Windows 实现 ──────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn run_event_loop() {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, TranslateMessage, MSG};

    unsafe {
        // 确保本线程获取物理像素坐标（不受缩放影响）
        let _ = windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );

        let tid = GetCurrentThreadId();
        WATCHER_THREAD_ID.store(tid, Ordering::SeqCst);

        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_changed),
            0,
            0,
            WINEVENT_SKIPOWNPROCESS, // 跳过本进程事件，回调在本线程（out-of-context）
        );

        if hook.0.is_null() {
            tracing::error!("游戏模式: SetWinEventHook 失败");
            GAME_MODE_ENABLED.store(false, Ordering::SeqCst);
            return;
        }

        tracing::info!("游戏模式: 事件监听已启动（零轮询）");

        // 启动时立即检测当前状态
        check_and_update();

        // 消息循环——GetMessageW 在无消息时阻塞，不消耗 CPU
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWinEvent(hook);
        WATCHER_THREAD_ID.store(0, Ordering::SeqCst);

        // 退出时若仍在抑制则恢复
        if GAME_MODE_SUPPRESSED.swap(false, Ordering::Relaxed) {
            if let Some(app) = GAME_MODE_APP.get() {
                restore_features(app);
            }
        }
        tracing::info!("游戏模式: 事件监听已退出");
    }
}

/// WinEvent 回调——仅在前台窗口切换时被系统调用
#[cfg(target_os = "windows")]
unsafe extern "system" fn on_foreground_changed(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    _hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _event_time: u32,
) {
    check_and_update();
}

/// 检测前台窗口是否全屏，按需切换抑制状态
#[cfg(target_os = "windows")]
fn check_and_update() {
    let app = match GAME_MODE_APP.get() {
        Some(a) => a,
        None => return,
    };

    let fullscreen = is_foreground_fullscreen();
    let was_suppressed = GAME_MODE_SUPPRESSED.load(Ordering::Relaxed);

    if fullscreen && !was_suppressed {
        suppress_features(app);
        GAME_MODE_SUPPRESSED.store(true, Ordering::Relaxed);
        tracing::info!("游戏模式: 检测到全屏应用，已暂停功能");
    } else if !fullscreen && was_suppressed {
        restore_features(app);
        GAME_MODE_SUPPRESSED.store(false, Ordering::Relaxed);
        tracing::info!("游戏模式: 全屏应用已退出，已恢复功能");
    }
}

/// 检测当前前台窗口是否为全屏应用（排除桌面和 Shell 窗口）
#[cfg(target_os = "windows")]
fn is_foreground_fullscreen() -> bool {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }

        // 排除桌面和 Shell 窗口
        if hwnd == GetDesktopWindow() || hwnd == GetShellWindow() {
            return false;
        }

        // 获取窗口矩形
        let mut window_rect = windows::Win32::Foundation::RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            return false;
        }

        // 比较窗口矩形与所在显示器矩形
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }

        let s = info.rcMonitor;
        window_rect.left <= s.left
            && window_rect.top <= s.top
            && window_rect.right >= s.right
            && window_rect.bottom >= s.bottom
    }
}

// ── 功能抑制 / 恢复 ──────────────────────────────────────────────────

/// 抑制功能：暂停剪贴板监控 + 注销所有快捷键
fn suppress_features(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.monitor.pause();
    }
    crate::disable_all_shortcuts(app);
}

/// 恢复功能：恢复剪贴板监控 + 重新注册快捷键（尊重用户手动禁用状态）
fn restore_features(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.monitor.resume();
    }
    if !crate::SHORTCUTS_DISABLED.load(Ordering::Relaxed) {
        crate::enable_all_shortcuts(app);
    }
}
