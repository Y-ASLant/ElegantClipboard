//! Windows 系统背板：亚克力（Acrylic）与云母（Mica）。
//!
//! 显式 DWM 系统背板覆盖整个窗口；保留 GPUI 创建的窗口样式和非客户区。
//! 不移除 caption/缩放边框，不接管 WM_NCCALCSIZE，也不修改系统圆角策略。
//! 客户区由 GPUI 透明交换链与应用外壳染色共同绘制。
//!
//! 材质只作用于窗口外壳，不修改组件主题。控件与浮层保留原有底色，
//! 避免半透明表面透出自身阴影；装饰性容器可局部透明以露出同一层外壳。
//!
//! 生效值通过 Global 广播：主窗口和设置窗口在创建时套用，
//! 偏好保存成功后推送给已打开的 Root 窗口；预览与拖拽浮层始终透明。

use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_HOSTBACKDROPBRUSH, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use ::windows::core::BOOL;
use clipboard_core::preferences::{WindowEffect, WindowEffectPreference};
use clipboard_platform::window_effect::{WindowEffectBackend, WindowEffectCapabilities};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Window, WindowBackgroundAppearance};

/// Resolve native capabilities before the first GPUI frame, then configure the window.
pub(crate) fn init_window_effect(
    preference: WindowEffectPreference,
    window: &mut Window,
    cx: &mut App,
) {
    let effect = WindowEffectCapabilities::current()
        .resolve(preference.effect)
        .effect();
    crate::window_effect::record(preference, effect, cx);
    apply(preference.effect, window);
}

/// Only a material change reconfigures native windows; tint changes just repaint.
pub(crate) fn apply_to_open_windows(effect: WindowEffect, window: &mut Window, cx: &mut App) {
    apply(effect, window);
    let current = window.window_handle().window_id();
    for handle in cx.windows() {
        // Rootless preview/drag layers must remain transparent, never a monitor-wide backdrop.
        if handle.window_id() == current || handle.downcast::<gpui_kit::base::Root>().is_none() {
            continue;
        }
        let _ = handle.update(cx, |_, window, _| apply(effect, window));
    }
}

/// Match the OS material palette to the app theme, without touching frame geometry.
pub(crate) fn sync_theme(window: &Window, cx: &mut App) {
    if !WindowEffectCapabilities::current().supports_system_backdrop() {
        return;
    }
    let dark = BOOL::from(cx.theme().is_dark());
    let apply = |window: &Window| {
        if let Some(hwnd) = super::window::window_hwnd(window) {
            // SAFETY: This attribute takes a BOOL, live for the duration of the call.
            if let Err(error) = unsafe { set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark) }
            {
                tracing::warn!(%error, "failed to match native material to app theme");
            }
        }
    };
    apply(window);
    let current = window.window_handle().window_id();
    for handle in cx.windows() {
        if handle.window_id() != current && handle.downcast::<gpui_kit::base::Root>().is_some() {
            let _ = handle.update(cx, |_, window, _| apply(window));
        }
    }
}

/// 首次开窗或有效材质变化时调用；遮罩强度不参与原生窗口配置。
pub(crate) fn apply(effect: WindowEffect, window: &mut Window) {
    let capabilities = WindowEffectCapabilities::current();
    let backend = capabilities.resolve(effect);
    // GPUI's Transparent appearance installs a WCA accent policy rather than
    // merely clearing alpha. Clear legacy accents before using the DWM backend.
    if capabilities.supports_system_backdrop() {
        window.set_background_appearance(WindowBackgroundAppearance::Opaque);
    }
    window.set_background_appearance(appearance(backend));
    let Some(hwnd) = super::window::window_hwnd(window) else {
        return;
    };
    if capabilities.supports_system_backdrop()
        && let Err(error) = set_dwm_attributes(hwnd, backend.effect())
    {
        tracing::error!(?effect, ?backend, %error, "failed to apply DWM background material");
    }
}

fn appearance(backend: WindowEffectBackend) -> WindowBackgroundAppearance {
    match backend {
        WindowEffectBackend::Opaque => WindowBackgroundAppearance::Opaque,
        WindowEffectBackend::LegacyAcrylic | WindowEffectBackend::NativeBlur => {
            WindowBackgroundAppearance::Blurred
        }
        // This appearance enables transparent GPUI rendering without a WCA accent.
        // The actual Acrylic/Mica backdrop is applied below through DWM.
        WindowEffectBackend::Acrylic | WindowEffectBackend::Mica => {
            WindowBackgroundAppearance::MicaBackdrop
        }
    }
}

fn backdrop(effect: WindowEffect) -> DWM_SYSTEMBACKDROP_TYPE {
    match effect {
        WindowEffect::Opaque => DWMSBT_NONE,
        WindowEffect::Frosted => DWMSBT_TRANSIENTWINDOW,
        WindowEffect::Mica => DWMSBT_MAINWINDOW,
    }
}

/// Keep the native frame, hit testing, shadow and corner policy intact.
/// Explicit backdrop types cover the whole window without extending the native caption.
fn set_dwm_attributes(hwnd: HWND, effect: WindowEffect) -> ::windows::core::Result<()> {
    let enabled = effect != WindowEffect::Opaque;
    let use_host_backdrop_brush = BOOL::from(enabled);
    let backdrop_type = backdrop(effect);
    // SAFETY: Values match their DWM attributes and live throughout each call.
    unsafe {
        set_attribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, &use_host_backdrop_brush)?;
        set_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop_type)?;
    }
    tracing::debug!(
        ?effect,
        "applied DWM material without changing native frame"
    );
    Ok(())
}

/// # Safety
///
/// `value` 在调用期间必须保持存活，且属性参数要求的布局与 `T` 一致。
unsafe fn set_attribute<T>(
    hwnd: HWND,
    attribute: DWMWINDOWATTRIBUTE,
    value: &T,
) -> ::windows::core::Result<()> {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            (value as *const T).cast(),
            std::mem::size_of::<T>() as u32,
        )
    }
}
