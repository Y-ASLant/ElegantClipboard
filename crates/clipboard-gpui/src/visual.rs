//! Shared spacing, control sizing and motion for the native UI.
use clipboard_core::preferences::CardDensity;
use gpui_kit::*;
use std::time::Duration;
pub const PAGE_PADDING: f32 = 8.;
pub const ROW_HEIGHT: f32 = 96.;
pub fn row_height(density: CardDensity, preview_lines: u8) -> f32 {
    let base = match density {
        CardDensity::Compact => 88.,
        CardDensity::Standard => ROW_HEIGHT,
        CardDensity::Spacious => 112.,
    };
    base + f32::from(preview_lines.saturating_sub(2)) * 20.
}

pub fn image_row_height(density: CardDensity) -> f32 {
    match density {
        CardDensity::Compact => 112.,
        CardDensity::Standard => 124.,
        CardDensity::Spacious => 148.,
    }
}

pub fn card_spacing(density: CardDensity) -> f32 {
    match density {
        CardDensity::Compact => 2.,
        CardDensity::Standard => 4.,
        CardDensity::Spacious => 6.,
    }
}

pub fn thumbnail_size(density: CardDensity) -> (f32, f32) {
    match density {
        CardDensity::Compact => (96., 56.),
        CardDensity::Standard => (120., 68.),
        CardDensity::Spacious => (140., 88.),
    }
}
pub const GROUP_BAR_HEIGHT: f32 = 42.;
pub const DROP_MARKER_HEIGHT: f32 = 2.;
pub const DRAG_EDGE_ZONE: f32 = 32.;
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

pub fn drag_yield(
    element: Div,
    from: f32,
    to: f32,
    id: impl Into<ElementId>,
    cx: &App,
) -> AnyElement {
    let element = element.relative();
    if cx.reduce_motion() || from == to {
        return element.top(px(to)).into_any_element();
    }
    element
        .with_animation(id, motion(), move |element, progress| {
            element.top(px(from + (to - from) * progress))
        })
        .into_any_element()
}
