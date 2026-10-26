pub use crate::popup_geometry::hover_popup_bounds;
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

const CURSOR_GAP: i32 = 12;

fn placement(
    mode: WindowPositionPreference,
    cursor: POINT,
    work: RECT,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let max_x = (work.right - width).max(work.left);
    let max_y = (work.bottom - height).max(work.top);
    match mode {
        WindowPositionPreference::FollowCursor => {
            let right = cursor.x + CURSOR_GAP;
            let x = if right + width > work.right {
                cursor.x - width - CURSOR_GAP
            } else {
                right
            };
            (
                x.clamp(work.left, max_x),
                (cursor.y + CURSOR_GAP).clamp(work.top, max_y),
            )
        }
        WindowPositionPreference::ScreenCenter => (
            (work.left + (work.right - work.left - width) / 2).clamp(work.left, max_x),
            (work.top + (work.bottom - work.top - height) / 2).clamp(work.top, max_y),
        ),
        WindowPositionPreference::FixedPosition => {
            unreachable!("fixed mode keeps the current position")
        }
    }
}

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
        cursor,
        info.rcWork,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_cursor_flips_and_clamps_within_work_area() {
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                POINT { x: -1000, y: 200 },
                work,
                500,
                600
            ),
            (-988, 212)
        );
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                POINT { x: -100, y: 900 },
                work,
                500,
                600
            ),
            (-612, 440)
        );
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                POINT { x: -1910, y: 10 },
                work,
                2200,
                1200
            ),
            (-1920, 0)
        );
    }

    #[test]
    fn screen_center_uses_selected_monitor_work_area() {
        let work = RECT {
            left: 1920,
            top: 40,
            right: 3840,
            bottom: 1080,
        };
        assert_eq!(
            placement(
                WindowPositionPreference::ScreenCenter,
                POINT { x: 2500, y: 500 },
                work,
                560,
                760
            ),
            (2600, 180)
        );
    }
}
