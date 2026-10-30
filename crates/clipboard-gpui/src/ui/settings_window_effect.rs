use super::settings_common::{
    choice_tabs, owner_state, settings_description, settings_field, settings_stack,
};
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
            WindowEffect::Opaque,
            WindowEffect::Frosted,
            WindowEffect::Mica,
        ]
        .into_iter()
        .filter(move |effect| capabilities.supports(*effect));
        let owner = self.owner.clone();
        let tabs = TabBar::new("settings-window-effect")
            .w_full()
            .min_w_0()
            .h(px(visual::CONTROL_HEIGHT_LARGE))
            .large()
            .segmented()
            .last_empty_space(div().w_0())
            .selected_index(
                choices
                    .clone()
                    .position(|effect| effect == backend.effect())
                    .unwrap_or(0),
            )
            .children(choices.clone().map(|effect| {
                Tab::new()
                    .label(effect_label(language, effect))
                    .text_size(px(visual::TEXT_BODY))
                    .disabled(pending)
                    .flex_1()
            }))
            .on_click(move |index: &usize, _, cx| {
                let Some(effect) = choices.clone().nth(*index) else {
                    return;
                };
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
        settings_stack()
            .child(settings_field(tr(language, "背景材质", "Background material")).child(tabs))
            .when(backend != WindowEffectBackend::Opaque, |content| {
                content.child(settings_field(tr(language, "背景浓度", "Background density"))
                    .child(choice_tabs(
                        "settings-window-effect-opacity",
                        &self.owner,
                        [40_u8, 55_u8, 72_u8, 85_u8],
                        ["40%", "55%", "72%", "85%"],
                        preference.opacity,
                        pending,
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
                    )))
                    .child(settings_description(tr(
                        language,
                        "数值越高，背景越实。窗口、侧栏与内容卡片共享材质层次；文字、控件和浮层保持清晰，不改变系统模糊半径。",
                        "Higher density makes the background more solid. The window, sidebar and content cards share material surfaces; text, controls and overlays stay clear. The system blur radius is unchanged.",
                    ), cx))
            })
            .when(!supported, |content| {
                let saved = effect_label(language, preference.effect);
                let actual = effect_label(language, backend.effect());
                let message = match language {
                    LanguagePreference::Chinese => format!(
                        "已保存的材质“{saved}”在本机不受支持，按平台能力使用“{actual}”。原偏好仍保留；选择上方选项后才会修改保存值。"
                    ),
                    LanguagePreference::English => format!(
                        "The saved material “{saved}” is unavailable on this platform, which uses “{actual}” instead. Your preference is retained until you choose a supported option above."
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
                    settings_description(tr(
                        language,
                        "当前通过 Windows 兼容模式呈现毛玻璃。",
                        "Frosted glass is using the Windows compatibility backend.",
                    ), cx),
                )
            })
            .when(
                preference.effect == WindowEffect::Opaque
                    && !capabilities.supports(WindowEffect::Frosted),
                |content| {
                    content.child(
                        settings_description(tr(
                            language,
                            "当前环境没有可用的原生背景模糊能力，使用不透明背景。",
                            "Native background blur is unavailable in this environment. The background remains opaque.",
                        ), cx),
                    )
                },
            )
            .into_any_element()
    }
}

fn effect_label(language: LanguagePreference, effect: WindowEffect) -> &'static str {
    match effect {
        WindowEffect::Opaque => tr(language, "无", "None"),
        WindowEffect::Frosted => tr(language, "毛玻璃", "Frosted glass"),
        WindowEffect::Mica => tr(language, "云母", "Mica"),
    }
}
