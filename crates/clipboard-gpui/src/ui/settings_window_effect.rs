use super::settings_common::{choice_tabs, owner_state};
use super::*;
use clipboard_platform::window_effect::{WindowEffectBackend, WindowEffectCapabilities};

impl SettingsWindowView {
    pub(super) fn settings_window_effect_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let preference = owner_state.window_effect;
        let pending = owner_state.window_effect_pending;
        let capabilities = WindowEffectCapabilities::current();
        let backend = capabilities.resolve(preference.effect);
        let supported = capabilities.supports(preference.effect);
        let choices = [
            WindowEffect::None,
            WindowEffect::Acrylic,
            WindowEffect::Mica,
            WindowEffect::MicaAlt,
        ]
        .into_iter()
        .filter(move |effect| capabilities.supports(*effect) || *effect == preference.effect);
        let owner = self.owner.clone();
        let tabs = TabBar::new("settings-window-effect")
            .w_full()
            .segmented()
            .selected_index(
                choices
                    .clone()
                    .position(|effect| effect == preference.effect)
                    .unwrap_or(0),
            )
            .children(choices.clone().map(|effect| {
                Tab::new()
                    .label(effect_label(language, effect))
                    .disabled(pending || !capabilities.supports(effect))
                    .flex_1()
            }))
            .on_click(move |index: &usize, _, cx| {
                let Some(effect) = choices.clone().nth(*index) else {
                    return;
                };
                if !capabilities.supports(effect) {
                    return;
                }
                let _ = owner.update(cx, |owner, cx| {
                    if !owner.window_effect_pending && owner.window_effect.effect != effect {
                        owner.save_window_effect(
                            WindowEffectPreference {
                                effect,
                                ..owner.window_effect
                            },
                            cx,
                        );
                    }
                });
            });
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(280.))
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "窗口效果", "Window effect"))
                    .child(div().w_full().min_w_0().child(tabs)),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(280.))
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "背景遮罩强度", "Background tint"))
                    .child(div().w_full().min_w_0().child(choice_tabs(
                        "settings-window-effect-opacity",
                        &self.owner,
                        [40_u8, 55_u8, 72_u8, 85_u8],
                        ["40%", "55%", "72%", "85%"],
                        preference.opacity,
                        pending || backend == WindowEffectBackend::Opaque,
                        |owner| owner.window_effect.opacity,
                        |owner, opacity, cx| {
                            owner.save_window_effect(
                                WindowEffectPreference {
                                    opacity,
                                    ..owner.window_effect
                                },
                                cx,
                            );
                        },
                    ))),
            )
            .when(!supported, |content| {
                let saved = effect_label(language, preference.effect);
                let actual = match backend {
                    WindowEffectBackend::Opaque => tr(language, "关闭（不透明）", "Off (opaque)"),
                    WindowEffectBackend::LegacyAcrylic => tr(
                        language,
                        "亚克力（Windows 10 兼容模式）",
                        "Acrylic (Windows 10 compatibility mode)",
                    ),
                    _ => effect_label(language, backend.effect()),
                };
                let message = match language {
                    LanguagePreference::Chinese => format!(
                        "已保存的选项“{saved}”在本机不受支持，实际降级为“{actual}”。已保留原设置；你可以选择上方受支持的选项。"
                    ),
                    LanguagePreference::English => format!(
                        "The saved option “{saved}” is not supported on this PC. The actual fallback is {actual}. Your saved setting is retained; you can choose a supported option above."
                    ),
                };
                content.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .child(Alert::warning("window-effect-fallback", message).small()),
                )
            })
            .when(supported && backend == WindowEffectBackend::LegacyAcrylic, |content| {
                content.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(
                            language,
                            "当前使用亚克力（Windows 10 兼容模式）。",
                            "Acrylic is running in Windows 10 compatibility mode.",
                        )),
                )
            })
            .when(
                preference.effect == WindowEffect::None
                    && !capabilities.supports(WindowEffect::Acrylic),
                |content| {
                    content.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .whitespace_normal()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(
                                language,
                                "当前系统不支持窗口材质，或无法确认系统版本；窗口效果已关闭，使用不透明背景。",
                                "Native window materials are unavailable on this desktop. Effects are off; the background is opaque.",
                            )),
                    )
                },
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "仅调整窗口背景的遮罩，不改变控件透明度或系统模糊半径。数值越高越接近主题纯色，越低越通透。",
                        "Only changes the window background tint, not control opacity or system blur. Higher values look more solid; lower values reveal more of the backdrop.",
                    )),
            )
            .into_any_element()
    }
}

fn effect_label(language: LanguagePreference, effect: WindowEffect) -> &'static str {
    match effect {
        WindowEffect::None => tr(language, "关闭", "Off"),
        WindowEffect::Acrylic if cfg!(target_os = "macos") => {
            tr(language, "系统毛玻璃", "Native blur")
        }
        WindowEffect::Acrylic => tr(language, "亚克力", "Acrylic"),
        WindowEffect::Mica => tr(language, "云母", "Mica"),
        WindowEffect::MicaAlt => tr(language, "云母 Alt", "Mica Alt"),
    }
}
