use super::*;

/// Only an effective material change reconfigures native windows; tint changes just repaint.
pub(super) fn apply_window_effect_to_open_windows(
    effect: WindowEffect,
    window: &mut Window,
    cx: &mut Context<ClipboardView>,
) {
    acrylic::apply(effect, window);
    let current = window.window_handle().window_id();
    for handle in cx.windows() {
        if handle.window_id() == current {
            continue;
        }
        let _ = handle.update(cx, |_, window, _| acrylic::apply(effect, window));
    }
}
