use anyhow::{Context, Result, anyhow};
use clipboard_core::preferences::{HoverPreviewPosition, WindowPositionPreference};
use gpui_kit::{Bounds, Pixels, Point, Window, point, px, size};
use std::mem::size_of;
use windows::Win32::{
    Foundation::{POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    },
};

const CURSOR_GAP: i32 = 12;

pub fn hover_popup_bounds(
    cursor: Point<Pixels>,
    display: Bounds<Pixels>,
    position: HoverPreviewPosition,
    image_dimensions: Option<(i64, i64)>,
) -> Bounds<Pixels> {
    let expanded = image_dimensions
        .filter(|(width, height)| (1..=50_000).contains(width) && (1..=50_000).contains(height));
    let width = expanded
        .map_or(px(460.), |(width, _)| px(width as f32 + 24.))
        .max(px(460.))
        .min(display.size.width);
    let height = expanded
        .map_or(px(300.), |(_, height)| px(height as f32 + 50.))
        .max(px(300.))
        .min(display.size.height);
    let right = display.origin.x + display.size.width;
    let bottom = display.origin.y + display.size.height;
    let right_x = cursor.x + px(18.);
    let left_x = cursor.x - width - px(18.);
    let right_fits = right_x + width <= right;
    let left_fits = left_x >= display.origin.x;
    let x = match position {
        HoverPreviewPosition::Left if left_fits => left_x,
        HoverPreviewPosition::Right if right_fits => right_x,
        HoverPreviewPosition::Left => right_x,
        HoverPreviewPosition::Right => left_x,
        HoverPreviewPosition::Auto if right_fits => right_x,
        HoverPreviewPosition::Auto => left_x,
    }
    .clamp(display.origin.x, right - width);
    let y = (cursor.y + px(18.)).clamp(display.origin.y, bottom - height);
    Bounds {
        origin: point(x, y),
        size: size(width, height),
    }
}

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
    let hwnd = crate::tray::window_hwnd(window).context("无法取得窗口句柄")?;
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

    #[test]
    fn expanded_image_window_uses_dimensions_without_leaving_display() {
        let display = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1920.), px(1080.)),
        };
        let cursor = point(px(1800.), px(900.));
        let standard = hover_popup_bounds(cursor, display, HoverPreviewPosition::Auto, None);
        assert_eq!(standard.size, size(px(460.), px(300.)));

        let expanded = hover_popup_bounds(
            cursor,
            display,
            HoverPreviewPosition::Auto,
            Some((1200, 800)),
        );
        assert_eq!(expanded.size, size(px(1224.), px(850.)));
        assert!(expanded.origin.x >= display.origin.x);
        assert!(expanded.origin.y >= display.origin.y);
        assert!(expanded.origin.x + expanded.size.width <= display.size.width);
        assert!(expanded.origin.y + expanded.size.height <= display.size.height);

        let oversized = hover_popup_bounds(
            cursor,
            display,
            HoverPreviewPosition::Auto,
            Some((5000, 5000)),
        );
        assert_eq!(oversized.size, display.size);
        assert_eq!(
            hover_popup_bounds(cursor, display, HoverPreviewPosition::Auto, Some((0, 800))).size,
            standard.size,
        );
    }
}
