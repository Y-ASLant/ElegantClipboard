use clipboard_core::preferences::HoverPreviewPosition;
#[cfg(any(windows, target_os = "linux"))]
use clipboard_core::preferences::WindowPositionPreference;
use gpui_kit::{Bounds, Pixels, point, px, size};
/// 未开启大图预览时的浮窗尺寸。
const DEFAULT_POPUP_SIZE: (f32, f32) = (460., 300.);
/// 大图预览的窗口最多占显示区域的比例。
///
/// 不设上限时，一张 1924×1082 的截图会开出比屏幕还大的窗口，把列表整个盖住——
/// 用户既看不到别的记录，也没法把鼠标移出去切换到别的图片，只能干瞪眼。
/// 留出余量后大图依然明显更大，但屏幕上始终有一块列表可见、可点。
const EXPANDED_MAX_SHARE: f32 = 0.7;
const EDGE_INSET: f32 = 12.;

pub fn hover_popup_bounds(
    owner: Bounds<Pixels>,
    display: Bounds<Pixels>,
    position: HoverPreviewPosition,
    image_dimensions: Option<(i64, i64)>,
    expanded_image: bool,
    anchor_y: Pixels,
) -> Option<Bounds<Pixels>> {
    let gap = px(18.);
    let inset = px(EDGE_INSET);
    let right = display.right() - inset;
    let bottom = display.bottom() - inset;
    let top = display.top() + inset;
    let owner_right = owner.right();
    let left_space = (owner.left() - gap - display.left() - inset).max(px(0.));
    let right_space = (right - owner_right - gap).max(px(0.));
    let (desired_width, desired_height) = popup_size(display, image_dimensions, expanded_image);
    let prefer_left = match position {
        HoverPreviewPosition::Left => true,
        HoverPreviewPosition::Right => false,
        HoverPreviewPosition::Auto => left_space > right_space,
    };
    let left = if prefer_left {
        left_space >= desired_width || left_space >= right_space
    } else {
        !(right_space >= desired_width || right_space >= left_space)
    };
    let available = if left { left_space } else { right_space };
    if bottom - top < px(16.) || right <= display.left() + inset {
        return None;
    }
    if available < px(400.).min(desired_width) {
        let scale = ((display.size.width - inset * 2.) / desired_width)
            .min((display.size.height - inset * 2.) * 0.8 / desired_height)
            .min(1.);
        let width = desired_width * scale;
        let height = desired_height * scale;
        let x = (owner.left() + (owner.size.width - width) / 2.).clamp(
            display.left() + inset,
            (right - width).max(display.left() + inset),
        );
        let y = if owner.top() - gap - height >= top {
            owner.top() - gap - height
        } else if owner.bottom() + gap + height <= bottom {
            owner.bottom() + gap
        } else {
            (anchor_y - height / 2.).clamp(top, (bottom - height).max(top))
        };
        return Some(Bounds::new(point(x, y), size(width, height)));
    }
    let scale = (available / desired_width)
        .min((bottom - top).min(display.size.height * 0.8) / desired_height)
        .min(1.);
    let width = desired_width * scale;
    let height = desired_height * scale;
    let x = if left {
        owner.origin.x - gap - width
    } else {
        owner_right + gap
    };
    Some(Bounds {
        origin: point(
            x,
            (anchor_y - height / 2.).clamp(top, (bottom - height).max(top)),
        ),
        size: size(width, height),
    })
}

#[derive(Clone, Copy)]
pub struct PopupZoomGeometry {
    pub initial: Bounds<Pixels>,
    pub owner: Bounds<Pixels>,
    pub display: Bounds<Pixels>,
    pub anchor_y: Pixels,
    /// Geometry is normalized to this source scale, including the desktop origin.
    pub scale_factor: f32,
}

impl PopupZoomGeometry {
    pub fn scaled(self, requested: f32) -> (Bounds<Pixels>, f32) {
        let inset = px(EDGE_INSET);
        let left = self.initial.right() <= self.owner.left();
        let right = self.initial.left() >= self.owner.right();
        let anchor = if left {
            self.initial.origin.x + self.initial.size.width
        } else {
            self.initial.origin.x
        };
        let available_width = if left {
            anchor - self.display.origin.x - inset
        } else if right {
            self.display.right() - inset - anchor
        } else {
            self.display.size.width - inset * 2.
        };
        let available_height = (self.display.size.height * 0.8)
            .min(self.display.size.height - inset * 2.)
            .max(px(1.));
        let image_width = self.initial.size.width;
        let image_height = self.initial.size.height;
        let longest = image_width.max(image_height).max(px(1.));
        let maximum = (available_width / image_width)
            .min(available_height / image_height)
            .min(px(2400.) / longest)
            .max(0.001);
        let zoom = requested.max(px(48.) / longest).min(maximum);
        let dimensions = size(image_width * zoom, image_height * zoom);
        let x = if left {
            anchor - dimensions.width
        } else if right {
            anchor
        } else {
            (self.initial.left() + self.initial.size.width / 2. - dimensions.width / 2.).clamp(
                self.display.left() + inset,
                (self.display.right() - inset - dimensions.width).max(self.display.left() + inset),
            )
        };
        let top = self.display.top() + inset;
        let y = (self.anchor_y - dimensions.height / 2.).clamp(
            top,
            (self.display.bottom() - inset - dimensions.height).max(top),
        );
        (Bounds::new(point(x, y), dimensions), zoom)
    }
}

/// Image surfaces follow the exact source aspect ratio; text keeps a readable fixed size.
fn popup_size(
    display: Bounds<Pixels>,
    image_dimensions: Option<(i64, i64)>,
    expanded_image: bool,
) -> (Pixels, Pixels) {
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
    let width = image_width as f32;
    let height = image_height as f32;
    let (max_width, max_height) = if expanded_image {
        (
            f32::from(display.size.width) * EXPANDED_MAX_SHARE,
            f32::from(display.size.height) * EXPANDED_MAX_SHARE,
        )
    } else {
        (480., 480.)
    };
    let scale = (max_width.max(1.) / width).min(max_height.max(1.) / height);
    (px(width * scale), px(height * scale))
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
mod tests;
