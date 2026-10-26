use ::windows::Win32::{
    Foundation::{POINT, RECT},
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

pub fn position_window(window: &Window, mode: WindowPositionPreference) -> Result<()> {
    if mode == WindowPositionPreference::FixedPosition
        || window.is_maximized()
        || window.is_fullscreen()
    {
        return Ok(());
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
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
        )
    }
    .context("无法移动窗口")?;
    Ok(())
}
