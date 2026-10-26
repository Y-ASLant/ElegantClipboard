use clipboard_core::preferences::HoverPreviewPosition;
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
}
