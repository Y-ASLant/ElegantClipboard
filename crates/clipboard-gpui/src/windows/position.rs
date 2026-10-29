use ::windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    },
};
use anyhow::{Context, Result, anyhow};
use clipboard_core::preferences::WindowPositionPreference;
use gpui_kit::Window;
use std::mem::size_of;

pub use crate::popup_geometry::placement;

#[derive(Debug)]
pub struct WindowPositionRequest {
    hwnd: HWND,
    x: i32,
    y: i32,
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
    let mut window_rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window_rect) }.context("无法读取窗口尺寸")?;
    let (x, y) = placement(
        mode,
        (cursor.x, cursor.y),
        (
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right,
            info.rcWork.bottom,
        ),
        window_rect.right - window_rect.left,
        window_rect.bottom - window_rect.top,
    );
    Ok(Some(WindowPositionRequest { hwnd, x, y }))
}

impl WindowPositionRequest {
    pub fn apply(self) -> Result<()> {
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                self.x,
                self.y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            )
        }
        .context("无法移动窗口")?;
        Ok(())
    }
}
