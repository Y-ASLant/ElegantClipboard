use clipboard_core::preferences::HoverPreviewPosition;
#[cfg(any(windows, target_os = "linux"))]
use clipboard_core::preferences::WindowPositionPreference;
use gpui_kit::{Bounds, Pixels, Point, point, px, size};
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

/// FollowCursor 模式下窗口边缘与光标的间距，Windows / X11 原实现均为 12。
#[cfg(any(windows, target_os = "linux"))]
const CURSOR_GAP: i32 = 12;

/// 主窗口摆放：top-left 原点整数像素坐标（x 向右、y 向下，Windows 与 X11 均如此）下的纯运算。
///
/// - `FollowCursor`：先放光标右下方、偏移 `CURSOR_GAP`；右侧放不下整体翻到左侧；x/y 都夹进工作区。
/// - `ScreenCenter`：在工作区内居中。
/// - `FixedPosition`：不在此处理（`position_window` 已提前返回）。
///
/// `work` 为 `(left, top, right, bottom)`，`cursor` 为指针位置，返回夹取后的左上角坐标。
/// macOS 的 Cocoa 坐标系 y 轴向上且光标坐标带小数，纵向运算与这里是镜像关系，
/// 不经由此函数（见 `unix/position.rs`）。
#[cfg(any(windows, target_os = "linux"))]
pub fn placement(
    mode: WindowPositionPreference,
    cursor: (i32, i32),
    work: (i32, i32, i32, i32),
    width: i32,
    height: i32,
) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    let max_x = (right - width).max(left);
    let max_y = (bottom - height).max(top);
    match mode {
        WindowPositionPreference::FollowCursor => {
            let right_x = cursor.0 + CURSOR_GAP;
            let x = if right_x + width > right {
                cursor.0 - width - CURSOR_GAP
            } else {
                right_x
            };
            (
                x.clamp(left, max_x),
                (cursor.1 + CURSOR_GAP).clamp(top, max_y),
            )
        }
        WindowPositionPreference::ScreenCenter => (
            (left + (right - left - width) / 2).clamp(left, max_x),
            (top + (bottom - top - height) / 2).clamp(top, max_y),
        ),
        WindowPositionPreference::FixedPosition => {
            unreachable!("fixed mode keeps the current position")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn follow_cursor_flips_and_clamps_within_work_area() {
        // 负坐标副屏：先放光标右下，放不下翻左，越界夹回工作区。
        let work = (-1920, 0, 0, 1040);
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                (-1000, 200),
                work,
                500,
                600
            ),
            (-988, 212)
        );
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                (-100, 900),
                work,
                500,
                600
            ),
            (-612, 440)
        );
        assert_eq!(
            placement(
                WindowPositionPreference::FollowCursor,
                (-1910, 10),
                work,
                2200,
                1200
            ),
            (-1920, 0)
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn screen_center_uses_selected_monitor_work_area() {
        // 居中取光标所在显示器的工作区，不是主屏的。
        assert_eq!(
            placement(
                WindowPositionPreference::ScreenCenter,
                (2500, 500),
                (1920, 40, 3840, 1080),
                560,
                760
            ),
            (2600, 180)
        );
    }
}
