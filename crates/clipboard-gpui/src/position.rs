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
/// 未开启大图预览时的浮窗尺寸。
const DEFAULT_POPUP_SIZE: (f32, f32) = (460., 300.);
/// 大图预览在图片尺寸之上额外留出的边距（横向 / 纵向，纵向要放得下标题与缩放按钮）。
const EXPANDED_PADDING: (f32, f32) = (24., 50.);
/// 大图预览的窗口最多占显示区域的比例。
///
/// 不设上限时，一张 1924×1082 的截图会开出比屏幕还大的窗口，把列表整个盖住——
/// 用户既看不到别的记录，也没法把鼠标移出去切换到别的图片，只能干瞪眼。
/// 留出余量后大图依然明显更大，但屏幕上始终有一块列表可见、可点。
const EXPANDED_MAX_SHARE: f32 = 0.7;

pub fn hover_popup_bounds(
    cursor: Point<Pixels>,
    display: Bounds<Pixels>,
    position: HoverPreviewPosition,
    image_dimensions: Option<(i64, i64)>,
) -> Bounds<Pixels> {
    let (width, height) = popup_size(display, image_dimensions);
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

/// 浮窗尺寸：默认 [`DEFAULT_POPUP_SIZE`]；有合法图片尺寸时按图片比例放大，
/// 但最多占显示区域的 [`EXPANDED_MAX_SHARE`]，等比收敛（`object_fit: Contain`
/// 会把图片缩进窗口，所以按比例缩窗口不会变形）。
fn popup_size(display: Bounds<Pixels>, image_dimensions: Option<(i64, i64)>) -> (Pixels, Pixels) {
    let (default_width, default_height) = DEFAULT_POPUP_SIZE;
    let floor = (
        px(default_width).min(display.size.width),
        px(default_height).min(display.size.height),
    );
    let Some((image_width, image_height)) = image_dimensions
        .filter(|(width, height)| (1..=50_000).contains(width) && (1..=50_000).contains(height))
    else {
        return floor;
    };
    let (pad_x, pad_y) = EXPANDED_PADDING;
    let desired_width = image_width as f32 + pad_x;
    let desired_height = image_height as f32 + pad_y;
    // 只缩不放：图片本来就小的时候保持自然尺寸，不硬撑到上限。
    let scale = (f32::from(display.size.width) * EXPANDED_MAX_SHARE / desired_width)
        .min(f32::from(display.size.height) * EXPANDED_MAX_SHARE / desired_height)
        .min(1.0);
    (
        px(desired_width * scale).max(floor.0),
        px(desired_height * scale).max(floor.1),
    )
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

        // 中等图片：比默认大，完整落在显示区域内，且保持图片长宽比。
        let expanded = hover_popup_bounds(
            cursor,
            display,
            HoverPreviewPosition::Auto,
            Some((1200, 800)),
        );
        assert!(expanded.size.width > standard.size.width);
        assert!(expanded.size.height > standard.size.height);
        assert!(expanded.origin.x >= display.origin.x);
        assert!(expanded.origin.y >= display.origin.y);
        assert!(expanded.origin.x + expanded.size.width <= display.size.width);
        assert!(expanded.origin.y + expanded.size.height <= display.size.height);
        let desired_aspect = (1200. + EXPANDED_PADDING.0) / (800. + EXPANDED_PADDING.1);
        let actual_aspect = f32::from(expanded.size.width) / f32::from(expanded.size.height);
        assert!((actual_aspect - desired_aspect).abs() < 0.001);

        // 超大图片：收敛到显示区域的一小部分，不再铺满整屏，长宽比同样保持。
        let oversized = hover_popup_bounds(
            cursor,
            display,
            HoverPreviewPosition::Auto,
            Some((5000, 5000)),
        );
        assert!(f32::from(oversized.size.width) <= 1920. * EXPANDED_MAX_SHARE + 1.);
        assert!(f32::from(oversized.size.height) <= 1080. * EXPANDED_MAX_SHARE + 1.);
        assert!(oversized.size.width > standard.size.width);
        assert!(
            oversized.origin.x + oversized.size.width <= display.size.width
                && oversized.origin.y + oversized.size.height <= display.size.height
        );

        // 尺寸非法（0）时退回默认尺寸。
        assert_eq!(
            hover_popup_bounds(cursor, display, HoverPreviewPosition::Auto, Some((0, 800))).size,
            standard.size,
        );
    }
}
