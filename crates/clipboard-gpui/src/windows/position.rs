use ::windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongW, GetWindowPlacement, GetWindowRect, IsIconic,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPlacement, SetWindowPos,
        WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED, WS_EX_TOOLWINDOW,
    },
};
use anyhow::{Context, Result, anyhow};
use clipboard_core::preferences::WindowPositionPreference;
use gpui_kit::Window;
use std::mem::size_of;

pub use crate::popup_geometry::{fit_window_size, placement};

#[derive(Debug)]
pub struct WindowPositionRequest {
    hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    /// True when width/height were shrunk to the target work area.
    resized: bool,
    restore: Option<WINDOWPLACEMENT>,
}

/// Compute while the GPUI window is borrowed; apply only after that borrow ends.
pub fn prepare_window_position(
    window: &Window,
    mode: WindowPositionPreference,
) -> Result<Option<WindowPositionRequest>> {
    if mode == WindowPositionPreference::FixedPosition
        || window.is_maximized()
        || window.is_fullscreen()
    {
        return Ok(None);
    }
    let hwnd = super::window::window_hwnd(window).context("无法取得窗口句柄")?;
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }.context("无法读取光标位置")?;
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(anyhow!("无法读取光标所在显示器"));
    }
    let (window_rect, mut restore) = if unsafe { IsIconic(hwnd) }.as_bool() {
        let mut saved = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowPlacement(hwnd, &mut saved) }.context("无法读取窗口恢复位置")?;
        if saved.flags.contains(WPF_RESTORETOMAXIMIZED) {
            return Ok(None);
        }
        (saved.rcNormalPosition, Some(saved))
    } else {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.context("无法读取窗口尺寸")?;
        (rect, None)
    };
    let original = (
        window_rect.right - window_rect.left,
        window_rect.bottom - window_rect.top,
    );
    let work = (
        info.rcWork.left,
        info.rcWork.top,
        info.rcWork.right,
        info.rcWork.bottom,
    );
    let (width, height) = fit_window_size(work, original.0, original.1);
    let resized = (width, height) != original;
    let (x, y) = placement(mode, (cursor.x, cursor.y), work, width, height);
    if let Some(saved) = &mut restore {
        // WINDOWPLACEMENT uses workspace coordinates except for tool windows.
        let tool_window =
            unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOOLWINDOW.0 != 0;
        let (left, top) = if tool_window {
            (x, y)
        } else {
            (
                x - (info.rcWork.left - info.rcMonitor.left),
                y - (info.rcWork.top - info.rcMonitor.top),
            )
        };
        saved.rcNormalPosition = RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
    }
    Ok(Some(WindowPositionRequest {
        hwnd,
        x,
        y,
        width,
        height,
        resized,
        restore,
    }))
}

impl WindowPositionRequest {
    /// Moves (and if needed resizes) the window. `Ok(true)` when size changed to fit the work area.
    pub fn apply(self) -> Result<bool> {
        if let Some(saved) = self.restore {
            // Update the restore rectangle without first exposing the old one.
            unsafe { SetWindowPlacement(self.hwnd, &saved) }.context("无法更新窗口恢复位置")?;
            return Ok(self.resized);
        }
        let flags = if self.resized {
            SWP_NOACTIVATE | SWP_NOZORDER
        } else {
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER
        };
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                self.x,
                self.y,
                self.width,
                self.height,
                flags,
            )
        }
        .context("无法移动窗口")?;
        Ok(self.resized)
    }
}
