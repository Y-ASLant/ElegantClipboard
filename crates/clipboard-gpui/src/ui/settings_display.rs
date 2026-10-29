use super::settings_common::{
    choice_tabs, owner_state, settings_description, settings_field, settings_stack,
};
use super::*;
use gpui_kit::component::{checkbox::Checkbox, input::NumberInput};

impl SettingsWindowView {
    pub(super) fn settings_appearance_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let language_pending = owner_state.language_pending;
        let theme = owner_state.theme;
        let theme_pending = owner_state.theme_pending;
        let corner_radius = owner_state.corner_radius;
        let legacy_radius = match corner_radius.value {
            CornerRadiusPreference::Fixed(value) if ![0, 4, 16].contains(&value) => Some(value),
            _ => None,
        };
        settings_stack()
            .child(
                settings_field(tr(language, "语言", "Language")).child(choice_tabs(
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
                )),
            )
            .child(
                settings_field(tr(language, "主题", "Theme")).child(choice_tabs(
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
                )),
            )
            .child(
                settings_field(tr(language, "圆角", "Corner radius")).child(choice_tabs(
                    "settings-window-corner-radius",
                    &self.owner,
                    [
                        CornerRadiusPreference::System,
                        CornerRadiusPreference::Fixed(0),
                        CornerRadiusPreference::Fixed(4),
                        CornerRadiusPreference::Fixed(16),
                    ],
                    [
                        tr(language, "跟随系统", "System"),
                        tr(language, "关闭", "Off"),
                        "4 px",
                        "16 px",
                    ],
                    corner_radius.value,
                    corner_radius.pending,
                    |owner| owner.corner_radius.value,
                    |owner, radius, cx| {
                        if owner.send(Command::SetCornerRadius(radius), cx) {
                            owner.corner_radius.pending = true;
                            cx.notify();
                        }
                    },
                ).when(legacy_radius.is_some(), |tabs| tabs.selected_index(usize::MAX))),
            )
            .when_some(legacy_radius, |content, value| {
                content.child(settings_description(match language {
                    LanguagePreference::Chinese => format!("当前保留旧设置：{value} px。选择上方选项后更新。"),
                    LanguagePreference::English => format!("Keeping the saved legacy radius: {value} px. Choose an option above to update it."),
                }, cx))
            })
            .child(self.settings_window_effect_content(cx))
            .into_any_element()
    }

    pub(super) fn settings_display_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let display = owner_state.display;
        let display_pending = owner_state.display_pending;
        let category_owner = self.owner.clone();
        let drag_indicator_owner = self.owner.clone();
        settings_stack()
            .child(
                settings_stack()
                    .gap_1()
                    .child(
                        Checkbox::new("show-category-filter")
                            .w_full()
                            .min_h(px(32.))
                            .py(px(6.))
                            .text_size(px(14.))
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
                            .w_full()
                            .min_h(px(32.))
                            .py(px(6.))
                            .text_size(px(14.))
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
                    ),
            )
            .child(
                settings_field(tr(language, "卡片密度", "Card density")).child(choice_tabs(
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
                )),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(tr(language, "卡片预览行数", "Card preview lines"))
                    .child(
                        div().w(px(160.)).flex_none().child(
                            NumberInput::new(&self.preview_lines_input)
                                .h(px(36.))
                                .disabled(display_pending),
                        ),
                    ),
            )
            .child(
                settings_stack().gap_1().children(
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
                            .w_full()
                            .min_h(px(32.))
                            .py(px(6.))
                            .text_size(px(14.))
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
                settings_field(tr(language, "时间格式", "Time format")).child(choice_tabs(
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
                )),
            )
            .child(
                settings_field(tr(language, "来源显示", "Source display")).child(choice_tabs(
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
                )),
            )
            .into_any_element()
    }
}
