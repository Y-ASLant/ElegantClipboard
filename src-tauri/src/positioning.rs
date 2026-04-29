//! 窗口定位：跟随光标 + 屏幕边界检测 + 置顶

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};
use tracing::debug;

/// 窗口唤醒定位模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionMode {
    /// 跟随光标（优先右下方，边缘钳位）
    FollowCursor,
    /// 当前显示器居中
    ScreenCenter,
    /// 保持上次位置（不重新定位）
    FixedPosition,
}

impl PositionMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "screen_center" => Self::ScreenCenter,
            "fixed_position" => Self::FixedPosition,
            // 兼容旧值
            "follow_cursor" | "cursor_right_below" | "cursor_top_aligned" => Self::FollowCursor,
            _ => Self::FollowCursor,
        }
    }
}

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::POINT;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// 获取当前光标位置
#[cfg(target_os = "windows")]
pub fn get_cursor_position() -> (i32, i32) {
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point).is_ok() {
            return (point.x, point.y);
        }
    }
    let (x, y) = crate::input_monitor::get_cursor_position();
    (x as i32, y as i32)
}

#[cfg(not(target_os = "windows"))]
pub fn get_cursor_position() -> (i32, i32) {
    let (x, y) = crate::input_monitor::get_cursor_position();
    (x as i32, y as i32)
}

/// 根据指定模式定位窗口
pub fn position_window(window: &WebviewWindow, mode: PositionMode) -> Result<(), String> {
    if mode == PositionMode::FixedPosition {
        debug!("Window position mode: fixed, skipping reposition");
        return Ok(());
    }

    let (cx, cy) = get_cursor_position();
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let monitor = get_monitor_at_cursor(window, cx, cy)?;
    let pos = match mode {
        PositionMode::FollowCursor => calc_follow_cursor(cx, cy, size, &monitor),
        PositionMode::ScreenCenter => calc_screen_center(size, &monitor),
        PositionMode::FixedPosition => unreachable!(),
    };
    window.set_position(pos).map_err(|e| e.to_string())?;
    debug!("Window positioned at ({}, {}) mode={:?}", pos.x, pos.y, mode);
    Ok(())
}

/// 强制置顶窗口（覆盖任务栏）
///
/// tao 的 set_always_on_top 不带 SWP_NOACTIVATE，
/// 对非焦点窗口（focusable=false）无法可靠置顶。
#[cfg(target_os = "windows")]
pub fn force_topmost(window: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, HWND_TOPMOST,
    };

    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd.0 as *mut _),
                Some(HWND_TOPMOST),
                0, 0, 0, 0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn force_topmost(_window: &WebviewWindow) {}

struct MonitorInfo {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

/// 查找光标所在的显示器
fn get_monitor_at_cursor(
    window: &WebviewWindow,
    cx: i32,
    cy: i32,
) -> Result<MonitorInfo, String> {
    if let Ok(monitors) = window.available_monitors() {
        for m in monitors {
            let pos = m.position();
            let size = m.size();
            let (mx, my) = (pos.x, pos.y);
            let (mw, mh) = (size.width as i32, size.height as i32);
            if cx >= mx && cx < mx + mw && cy >= my && cy < my + mh {
                return Ok(MonitorInfo { x: mx, y: my, width: mw, height: mh });
            }
        }
    }
    if let Ok(Some(m)) = window.primary_monitor() {
        return Ok(MonitorInfo {
            x: m.position().x,
            y: m.position().y,
            width: m.size().width as i32,
            height: m.size().height as i32,
        });
    }
    Ok(MonitorInfo { x: 0, y: 0, width: 1920, height: 1080 })
}

/// 跟随光标：以光标为中心显示窗口，边缘钳位确保不超出显示器
fn calc_follow_cursor(
    cx: i32,
    cy: i32,
    window_size: PhysicalSize<u32>,
    m: &MonitorInfo,
) -> PhysicalPosition<i32> {
    let (w, h) = (window_size.width as i32, window_size.height as i32);

    // 以光标为中心
    let x = cx - w / 2;
    let y = cy - h / 2;

    // 钳位到显示器范围内
    let x = x.max(m.x).min(m.x + m.width - w);
    let y = y.max(m.y).min(m.y + m.height - h);

    PhysicalPosition::new(x, y)
}

/// 窗口在当前显示器居中
fn calc_screen_center(
    window_size: PhysicalSize<u32>,
    m: &MonitorInfo,
) -> PhysicalPosition<i32> {
    let (w, h) = (window_size.width as i32, window_size.height as i32);
    let x = m.x + (m.width - w) / 2;
    let y = m.y + (m.height - h) / 2;
    PhysicalPosition::new(x, y)
}
