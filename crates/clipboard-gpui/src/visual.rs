//! Shared spacing, control sizing and motion for the native UI.
use gpui_kit::*;
use std::time::Duration;
pub const PAGE_PADDING: f32 = 20.;
pub const ROW_HEIGHT: f32 = 160.;
pub const CONTROL_HEIGHT: f32 = 28.;
pub const FEEDBACK_DURATION: Duration = Duration::from_millis(160);
pub fn reveal(element: Div, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .with_animation(
            id,
            Animation::new(FEEDBACK_DURATION).with_easing(ease_out_quint()),
            |element, progress| element.opacity(0.65 + 0.35 * progress),
        )
        .into_any_element()
}
