use super::settings_common::choice_tabs;
use super::*;
use gpui_kit::component::{checkbox::Checkbox, input::NumberInput};

impl SettingsWindowView {
    pub(super) fn settings_appearance_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let language_pending = owner_state.language_pending;
        let theme = owner_state.theme;
        let theme_pending = owner_state.theme_pending;
        let window_effect = owner_state.window_effect;
        let window_effect_pending = owner_state.window_effect_pending;
        div()
            .w_full()
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
                    .child(tr(language, "语言", "Language"))
                    .child(div().flex_1().min_w_0().child(choice_tabs(
                        "settings-window-language",
                        &self.owner,
                        [LanguagePreference::Chinese, LanguagePreference::English],
                        ["简体中文", "English"],
                        language,
                        language_pending,
                        |owner| owner.language,
                        |owner, preference, cx| {
                            if owner.send(Command::SetLanguage(preference), cx) {
                                owner.language_pending = true;
                                cx.notify();
                            }
                        },
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(280.))
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "主题", "Theme"))
                    .child(div().flex_1().min_w_0().child(choice_tabs(
                        "settings-window-theme",
                        &self.owner,
                        [
                            ThemePreference::System,
                            ThemePreference::Light,
                            ThemePreference::Dark,
                        ],
                        [
                            tr(language, "跟随系统", "System"),
                            tr(language, "浅色", "Light"),
                            tr(language, "深色", "Dark"),
                        ],
                        theme,
                        theme_pending,
                        |owner| owner.theme,
                        |owner, preference, cx| {
                            if owner.send(Command::SetTheme(preference), cx) {
                                owner.theme_pending = true;
                                cx.notify();
                            }
                        },
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(320.))
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "窗口效果", "Window effect"))
                    .child(div().flex_1().min_w(px(320.)).child(choice_tabs(
                        "settings-window-effect",
                        &self.owner,
                        [
                            WindowEffect::None,
                            WindowEffect::Acrylic,
                            WindowEffect::Mica,
                            WindowEffect::MicaAlt,
                        ],
                        [
                            tr(language, "关闭", "Off"),
                            tr(language, "亚克力", "Acrylic"),
                            tr(language, "云母", "Mica"),
                            tr(language, "云母 Alt", "Mica Alt"),
                        ],
                        window_effect.effect,
                        window_effect_pending,
                        |owner| owner.window_effect.effect,
                        |owner, effect, cx| {
                            owner.save_window_effect(
                                WindowEffectPreference {
                                    effect,
                                    ..owner.window_effect
                                },
                                cx,
                            );
                        },
                    ))),
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
                    .child(div().flex_1().min_w_0().child(choice_tabs(
                        "settings-window-effect-opacity",
                        &self.owner,
                        [40_u8, 55_u8, 72_u8, 85_u8],
                        ["40%", "55%", "72%", "85%"],
                        window_effect.opacity,
                        window_effect_pending || window_effect.effect == WindowEffect::None,
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

    pub(super) fn settings_display_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let display = owner_state.display;
        let display_pending = owner_state.display_pending;
        let category_owner = self.owner.clone();
        let drag_indicator_owner = self.owner.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(
                Checkbox::new("show-category-filter")
                    .text_sm()
                    .label(tr(language, "显示分类筛选", "Show category filters"))
                    .checked(display.show_category_filter)
                    .disabled(display_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = category_owner.update(cx, |owner, cx| {
                            owner.save_display(
                                DisplayPreference {
                                    show_category_filter: *checked,
                                    ..owner.display
                                },
                                cx,
                            );
                        });
                    }),
            )
            .child(
                Checkbox::new("show-drag-area-indicator")
                    .text_sm()
                    .label(tr(language, "显示卡片拖动区域", "Show card drag areas"))
                    .checked(display.show_drag_area_indicator)
                    .disabled(display_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = drag_indicator_owner.update(cx, |owner, cx| {
                            owner.save_display(
                                DisplayPreference {
                                    show_drag_area_indicator: *checked,
                                    ..owner.display
                                },
                                cx,
                            );
                        });
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "卡片密度", "Card density")),
            )
            .child(choice_tabs(
                "card-density",
                &self.owner,
                [
                    CardDensity::Compact,
                    CardDensity::Standard,
                    CardDensity::Spacious,
                ],
                [
                    tr(language, "紧凑", "Compact"),
                    tr(language, "标准", "Standard"),
                    tr(language, "宽松", "Spacious"),
                ],
                display.card_density,
                display_pending,
                |owner| owner.display.card_density,
                |owner, density, cx| {
                    owner.save_display(
                        DisplayPreference {
                            card_density: density,
                            ..owner.display
                        },
                        cx,
                    );
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "卡片预览行数", "Card preview lines"))
                    .child(
                        div().w(px(120.)).flex_none().child(
                            NumberInput::new(&self.preview_lines_input)
                                .small()
                                .disabled(display_pending),
                        ),
                    ),
            )
            .child(
                div().flex().flex_wrap().gap_2().children(
                    [
                        (
                            "show-card-time",
                            tr(language, "显示时间", "Show time"),
                            display.show_time,
                            (|next: &mut DisplayPreference, checked| next.show_time = checked)
                                as fn(&mut DisplayPreference, bool),
                        ),
                        (
                            "show-card-characters",
                            tr(language, "显示字符数", "Show character count"),
                            display.show_char_count,
                            |next: &mut DisplayPreference, checked| next.show_char_count = checked,
                        ),
                        (
                            "show-card-size",
                            tr(language, "显示大小", "Show size"),
                            display.show_byte_size,
                            |next: &mut DisplayPreference, checked| next.show_byte_size = checked,
                        ),
                        (
                            "show-card-source",
                            tr(language, "显示来源应用", "Show source app"),
                            display.show_source_app,
                            |next: &mut DisplayPreference, checked| next.show_source_app = checked,
                        ),
                    ]
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        let owner = self.owner.clone();
                        Checkbox::new(id)
                            .text_sm()
                            .label(label)
                            .checked(selected)
                            .disabled(display_pending)
                            .on_change(move |checked, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let mut next = owner.display;
                                    set_checked(&mut next, *checked);
                                    owner.save_display(next, cx);
                                });
                            })
                    }),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "时间格式", "Time format")),
            )
            .child(choice_tabs(
                "time-format",
                &self.owner,
                [TimeFormat::Absolute, TimeFormat::Relative],
                [
                    tr(language, "绝对时间", "Absolute"),
                    tr(language, "相对时间", "Relative"),
                ],
                display.time_format,
                display_pending || !display.show_time,
                |owner| owner.display.time_format,
                |owner, format, cx| {
                    owner.save_display(
                        DisplayPreference {
                            time_format: format,
                            ..owner.display
                        },
                        cx,
                    );
                },
            ))
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "来源显示", "Source display")),
            )
            .child(choice_tabs(
                "source-app-display",
                &self.owner,
                [
                    SourceAppDisplay::Both,
                    SourceAppDisplay::Name,
                    SourceAppDisplay::Icon,
                ],
                [
                    tr(language, "名称和图标", "Name and icon"),
                    tr(language, "仅名称", "Name only"),
                    tr(language, "仅图标", "Icon only"),
                ],
                display.source_app_display,
                display_pending || !display.show_source_app,
                |owner| owner.display.source_app_display,
                |owner, mode, cx| {
                    owner.save_display(
                        DisplayPreference {
                            source_app_display: mode,
                            ..owner.display
                        },
                        cx,
                    );
                },
            ))
            .into_any_element()
    }
}
