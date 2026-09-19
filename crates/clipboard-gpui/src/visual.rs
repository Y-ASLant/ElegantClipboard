//! Shared spacing, control sizing and motion for the native UI.
use gpui_kit::*;
use std::time::Duration;
pub const PAGE_PADDING: f32 = 20.;
pub const ROW_HEIGHT: f32 = 116.;
pub const CONTROL_HEIGHT: f32 = 28.;
pub const GROUP_BAR_HEIGHT: f32 = 42.;
pub const DROP_MARKER_HEIGHT: f32 = 2.;
pub const THUMBNAIL_WIDTH: f32 = 48.;
pub const THUMBNAIL_HEIGHT: f32 = 36.;
pub const DRAG_EDGE_ZONE: f32 = 32.;
pub const GROUP_DRAG_SCROLL_STEP: f32 = 18.;
pub const DRAG_SCROLL_INTERVAL: Duration = Duration::from_millis(60);
pub const HISTORY_DRAG_SCROLL_TICKS: usize = 2;
pub const MOTION_DURATION: Duration = Duration::from_millis(180);

fn motion() -> Animation {
    Animation::new(MOTION_DURATION).with_easing(ease_out_quint())
}

pub fn reveal(element: Div, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .with_animation(id, motion(), |element, progress| {
            element.opacity(0.65 + 0.35 * progress)
        })
        .into_any_element()
}

pub fn reflow(element: Div, offset: f32, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .relative()
        .with_animation(id, motion(), move |element, progress| {
            element.top(px(offset * (1. - progress)))
        })
        .into_any_element()
}
