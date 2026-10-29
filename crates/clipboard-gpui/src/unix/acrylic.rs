use clipboard_core::preferences::{WindowEffect, WindowEffectPreference};
use clipboard_platform::window_effect::{WindowEffectBackend, WindowEffectCapabilities};
use gpui_kit::{App, Window, WindowBackgroundAppearance};

pub fn init_window_effect(preference: WindowEffectPreference, window: &mut Window, cx: &mut App) {
    let effect = WindowEffectCapabilities::current()
        .resolve(preference.effect)
        .effect();
    crate::window_effect::record(preference, effect, cx);
    apply(preference.effect, window);
}

pub fn apply(effect: WindowEffect, window: &mut Window) {
    let backend = WindowEffectCapabilities::current().resolve(effect);
    window.set_background_appearance(match backend {
        WindowEffectBackend::Opaque => WindowBackgroundAppearance::Opaque,
        WindowEffectBackend::NativeBlur
        | WindowEffectBackend::LegacyAcrylic
        | WindowEffectBackend::Acrylic
        | WindowEffectBackend::Mica => WindowBackgroundAppearance::Blurred,
    });
}

pub fn apply_to_open_windows(effect: WindowEffect, window: &mut Window, cx: &mut App) {
    apply(effect, window);
    let current = window.window_handle().window_id();
    for handle in cx.windows() {
        // Only application Root windows receive materials; passive overlays stay transparent.
        if handle.window_id() != current && handle.downcast::<gpui_kit::base::Root>().is_some() {
            let _ = handle.update(cx, |_, window, _| apply(effect, window));
        }
    }
}
