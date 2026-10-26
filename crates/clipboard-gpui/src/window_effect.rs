//! GPUI shell appearance and shared effect state, independent of native window setup.

use clipboard_core::preferences::{WindowEffect, WindowEffectPreference};
use gpui_kit::base::{Root, RootPlugin};
use gpui_kit::{
    App, Context, Div, Global, Hsla, IntoElement, Render, Stateful, StyleRefinement, Window, div,
    prelude::*, transparent_black,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct WindowEffectSetting(pub(crate) WindowEffectPreference);

impl Global for WindowEffectSetting {}

/// 保存已由平台能力解析的实际效果；用户请求值仍留在视图和 SQLite 中。
pub(crate) fn record(preference: WindowEffectPreference, effect: WindowEffect, cx: &mut App) {
    cx.set_global(WindowEffectSetting(WindowEffectPreference {
        effect,
        ..preference
    }));
}

pub(crate) fn active(cx: &App) -> WindowEffectPreference {
    cx.try_global::<WindowEffectSetting>()
        .map_or_else(WindowEffectPreference::default, |setting| setting.0)
}

/// 在应用初始化之后注册，晚于 gpui-component 的 `WindowState`。
///
/// GPUI 的直接混合状态是 `SrcBlendAlpha = ONE, DestBlendAlpha = ONE`，即 **alpha
/// 加法累积**：任何一层不透明 quad 一旦落笔，整个客户区的 alpha 就被钉死成 1，
/// 之后无论叠多少半透明层都救不回来。gpui-component 的 `WindowState::style` 恰恰
/// 给根 div 刷了一层不透明 `theme().tokens.background`，表现就是「边框透出背板、
/// 内容区纯黑」——DComp 只看到 alpha=1。
///
/// 根插件按注册顺序依次 `refine`，后注册者覆盖先注册者，所以这里在效果开启时把根背景
/// 改写为**全透明**：GPUI 的 `paint_quad` 会直接跳过透明背景，alpha 不再被钉死。
struct AcrylicRootPlugin;

impl Render for AcrylicRootPlugin {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RootPlugin for AcrylicRootPlugin {
    fn style(&self, surface: &mut Stateful<Div>, _: &mut Window, cx: &mut App) {
        if active(cx).effect == WindowEffect::None {
            return;
        }
        surface
            .style()
            .refine(&StyleRefinement::default().bg(transparent_black()));
    }
}

/// 注册根插件。必须在 `gpui_kit::init` 之后调用才有覆盖 `WindowState` 的资格，
/// 且只影响之后创建的窗口。
pub(crate) fn register_root_plugin(cx: &mut App) {
    Root::register_plugin::<AcrylicRootPlugin>(cx, |_, _| AcrylicRootPlugin);
}

/// 外壳只绘制一层主题染色；强度不分摊给控件，也不改变 DWM 的模糊半径。
pub(crate) fn shell_background(base: Hsla, preference: WindowEffectPreference) -> Hsla {
    if preference.effect == WindowEffect::None {
        return base;
    }
    Hsla {
        a: f32::from(preference.opacity) / 100.0,
        ..base
    }
}
