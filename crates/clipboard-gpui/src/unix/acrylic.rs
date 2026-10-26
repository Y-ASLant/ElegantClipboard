use clipboard_core::preferences::{WindowEffect, WindowEffectPreference};
use clipboard_platform::window_effect::WindowEffectCapabilities;
use gpui_kit::{App, Window, WindowBackgroundAppearance};

pub fn init_window_effect(preference: WindowEffectPreference, window: &mut Window, cx: &mut App) {
    let effect = WindowEffectCapabilities::current()
        .resolve(preference.effect)
        .effect();
    crate::window_effect::record(preference, effect, cx);
    apply(preference.effect, window);
}

pub fn apply(effect: WindowEffect, window: &mut Window) {
    let effect = WindowEffectCapabilities::current().resolve(effect).effect();
    window.set_background_appearance(if effect == WindowEffect::None {
        WindowBackgroundAppearance::Opaque
    } else {
        WindowBackgroundAppearance::Blurred
    });
}

pub fn apply_to_open_windows(effect: WindowEffect, window: &mut Window, cx: &mut App) {
    apply(effect, window);
    let current = window.window_handle().window_id();
    for handle in cx.windows() {
        if handle.window_id() != current {
            let _ = handle.update(cx, |_, window, _| apply(effect, window));
        }
    }
}
