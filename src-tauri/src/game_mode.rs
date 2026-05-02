//! 游戏模式：检测全屏应用时自动暂停剪贴板监控和全局快捷键。
//!
//! 使用 Windows `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` 事件驱动，
//! 辅以定时重检（3 秒），确保独占全屏等场景也能可靠检测。

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
/// 代际计数器，用于区分不同的 start/stop 周期，防止旧线程干扰新线程
static GENERATION: AtomicU32 = AtomicU32::new(0);

// Windows 常量
#[cfg(target_os = "windows")]
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
#[cfg(target_os = "windows")]
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
#[cfg(target_os = "windows")]
const RECHECK_INTERVAL_MS: u32 = 3000;

/// 启动游戏模式检测
pub fn start(app: tauri::AppHandle) {
    if GAME_MODE_ENABLED.swap(true, Ordering::SeqCst) {
        return; // 已在运行
    }
    let _ = GAME_MODE_APP.set(app);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    std::thread::Builder::new()
        .name("game-mode-watcher".into())
        .spawn(move || {
            #[cfg(target_os = "windows")]
            run_event_loop(generation);
        })
        .expect("failed to spawn game-mode-watcher thread");
}

/// 停止游戏模式检测
pub fn stop() {
    if !GAME_MODE_ENABLED.swap(false, Ordering::SeqCst) {
        return; // 未在运行
    }

    // 递增代际，使旧线程退出时不再执行恢复
    GENERATION.fetch_add(1, Ordering::SeqCst);

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

    // 如果当前处于抑制状态，立即恢复
    if GAME_MODE_SUPPRESSED.swap(false, Ordering::Relaxed) {
        if let Some(app) = GAME_MODE_APP.get() {
            restore_features(app);
            tracing::info!("游戏模式: 已停止，功能已恢复");
        }
    }
}

// ── Windows 实现 ──────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn run_event_loop(generation: u32) {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG,
        SetTimer, KillTimer, WM_TIMER,
    };

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

        // 定时重检：某些独占全屏游戏可能不触发 EVENT_SYSTEM_FOREGROUND
        let timer_id = SetTimer(None, 1, RECHECK_INTERVAL_MS, None);

        tracing::info!("游戏模式: 事件监听已启动（事件驱动 + {}s 定时重检）", RECHECK_INTERVAL_MS / 1000);

        // 启动时立即检测当前状态
        check_and_update();

        // 消息循环——GetMessageW 在无消息时阻塞，不消耗 CPU
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER {
                check_and_update();
            } else {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let _ = KillTimer(None, timer_id);
        let _ = UnhookWinEvent(hook);
        WATCHER_THREAD_ID.store(0, Ordering::SeqCst);

        // 退出时：仅当代际匹配时才恢复功能（防止旧线程覆盖新线程的状态）
        if GENERATION.load(Ordering::SeqCst) == generation {
            if GAME_MODE_SUPPRESSED.swap(false, Ordering::Relaxed) {
                if let Some(app) = GAME_MODE_APP.get() {
                    restore_features(app);
                }
            }
        }
        tracing::info!("游戏模式: 事件监听已退出 (generation={})", generation);
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
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
        GetWindowLongW, GWL_STYLE, GWL_EXSTYLE,
        WS_MINIMIZE, WS_EX_TOPMOST,
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

        // 排除过渡窗口（ForegroundStaging 等）
        let mut class_buf = [0u16; 256];
        let class_len = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
        if class_name == "ForegroundStaging" || class_name == "MultitaskingViewFrame" {
            return false;
        }

        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // 策略1: 独占全屏——窗口被最小化（坐标为 -32000）但仍是前台窗口且为最顶层
        if (style & WS_MINIMIZE.0 != 0) && (ex_style & WS_EX_TOPMOST.0 != 0) {
            return true;
        }

        // 获取窗口矩形
        let mut window_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err() {
            return false;
        }

        // 策略2: 独占全屏——窗口坐标在屏幕外（-32000 区域）
        if window_rect.left <= -30000 && window_rect.top <= -30000 {
            return true;
        }

        // 获取显示器矩形
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let s = info.rcMonitor;
        let mon_w = s.right - s.left;
        let mon_h = s.bottom - s.top;

        // 策略3: 标准全屏——窗口矩形覆盖整个显示器
        if window_rect.left <= s.left
            && window_rect.top <= s.top
            && window_rect.right >= s.right
            && window_rect.bottom >= s.bottom
        {
            return true;
        }

        // 策略4: 无边框全屏窗口可能有阴影/扩展帧，使用 DwmGetWindowAttribute
        // 获取实际可见区域（不含阴影）
        let mut frame_rect = RECT::default();
        let hr = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame_rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );
        if hr.is_ok() {
            if frame_rect.left <= s.left
                && frame_rect.top <= s.top
                && frame_rect.right >= s.right
                && frame_rect.bottom >= s.bottom
            {
                return true;
            }
        }

        // 策略5: 宽松匹配——窗口尺寸接近显示器尺寸（±16 像素容差）
        let win_w = window_rect.right - window_rect.left;
        let win_h = window_rect.bottom - window_rect.top;
        if win_w >= mon_w - 16 && win_h >= mon_h - 16
            && win_w <= mon_w + 16 && win_h <= mon_h + 16
        {
            return true;
        }

        false
    }
}

// ── 功能抑制 / 恢复 ──────────────────────────────────────────────────

/// 抑制功能：暂停剪贴板监控 + 禁用所有快捷键
fn suppress_features(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.monitor.pause();
    }
    crate::hotkey::disable_all();
}

/// 恢复功能：恢复剪贴板监控 + 重新启用快捷键（尊重用户手动禁用状态）
fn restore_features(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        state.monitor.resume();
    }
    if !crate::SHORTCUTS_DISABLED.load(Ordering::Relaxed) {
        crate::hotkey::enable_all();
    }
}
