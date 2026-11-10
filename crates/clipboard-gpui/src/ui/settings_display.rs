use super::category_tabs::{CATEGORY_TAB_SPECS, category_tab_flags, set_category_tab_visible};
use super::settings_common::{
    choice_tabs, owner_state, pref_checkbox, settings_column, settings_column_field,
    settings_column_field_help, settings_columns, settings_description, settings_field,
    settings_field_help, settings_inline, settings_stack, settings_toggle,
};
use super::*;
use clipboard_core::preferences::CardCornerPreference;
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
        let theme_color = owner_state.theme_color;
        let theme_color_pending = owner_state.theme_color_pending;
        let corner_radius = owner_state.corner_radius;
        let card_corner = owner_state.card_corner;
        let legacy_radius = match corner_radius.value {
            CornerRadiusPreference::Fixed(value) if ![0, 4, 16].contains(&value) => Some(value),
            _ => None,
        };
        settings_columns()
            .child(
                settings_column_field(tr(language, "语言", "Language")).child(choice_tabs(
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
                    cx,
                )),
            )
            .child(
                settings_column_field(tr(language, "主题", "Theme")).child(choice_tabs(
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
                    cx,
                )),
            )
            .child(
                settings_column_field_help(
                    tr(language, "主题色", "Theme color"),
                    tr(
                        language,
                        "主按钮、链接和悬停连线使用的颜色",
                        "Color used by primary buttons, links, and the hover connector",
                    ),
                    "theme-color-help",
                )
                    .child(choice_tabs(
                        "settings-theme-color",
                        &self.owner,
                        [
                            ThemeColor::VibrantOrange,
                            ThemeColor::Cyan,
                            ThemeColor::GoogleBlue,
                        ],
                        [
                            tr(language, "活力橙", "Vibrant orange"),
                            tr(language, "青色", "Cyan"),
                            tr(language, "谷歌蓝", "Google blue"),
                        ],
                        theme_color,
                        theme_color_pending,
                        |owner| owner.theme_color,
                        |owner, preference, cx| {
                            if owner.send(Command::SetThemeColor(preference), cx) {
                                owner.theme_color_pending = true;
                                cx.notify();
                            }
                        },
                        cx,
                    )),
            )
            .child(
                settings_column_field(tr(language, "圆角", "Corner radius")).child(choice_tabs(
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
                    cx,
                ).when(legacy_radius.is_some(), |tabs| tabs.selected_index(usize::MAX))),
            )
            .when_some(legacy_radius, |content, value| {
                content.child(settings_description(match language {
                    LanguagePreference::Chinese => format!("当前保留旧设置：{value} px。选择上方选项后更新。"),
                    LanguagePreference::English => format!("Keeping the saved legacy radius: {value} px. Choose an option above to update it."),
                }, cx))
            })
            .child(
                settings_column_field_help(
                    tr(language, "卡片圆角", "Card corners"),
                    tr(
                        language,
                        "剪贴板卡片和预览卡片。默认是全局圆角的一半。",
                        "Clipboard cards and preview cards. Default is half the global radius.",
                    ),
                    "card-corner-help",
                )
                    .child(choice_tabs(
                        "settings-card-corner",
                        &self.owner,
                        [
                            CardCornerPreference::Half,
                            CardCornerPreference::Global,
                            CardCornerPreference::Off,
                        ],
                        [
                            tr(language, "默认", "Default"),
                            tr(language, "跟随全局", "Global"),
                            tr(language, "关闭", "Off"),
                        ],
                        card_corner.value,
                        card_corner.pending,
                        |owner| owner.card_corner.value,
                        |owner, preference, cx| {
                            if owner.send(Command::SetCardCorner(preference), cx) {
                                owner.card_corner.pending = true;
                                cx.notify();
                            }
                        },
                        cx,
                    )),
            )
            .child(settings_column().child(self.settings_window_effect_content(cx)))
            .child(self.settings_fonts_content(cx))
            .into_any_element()
    }

    pub(super) fn settings_display_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let display = owner_state.display.value;
        let display_pending = owner_state.display.pending;
        // 分段条、数字框和复选框高度不同，各占一行；同一行只放同类复选框。
        settings_stack()
            .child(
                settings_inline().children(
                    [
                        (
                            "show-category-filter",
                            tr(language, "显示分类筛选", "Show category filters"),
                            display.show_category_filter,
                            (|next: &mut DisplayPreference, checked| {
                                next.show_category_filter = checked
                            }) as fn(&mut DisplayPreference, bool),
                        ),
                        (
                            "show-drag-area-indicator",
                            tr(language, "显示卡片拖动区域", "Show card drag areas"),
                            display.show_drag_area_indicator,
                            |next: &mut DisplayPreference, checked| {
                                next.show_drag_area_indicator = checked
                            },
                        ),
                    ]
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        pref_checkbox(
                            cx,
                            id,
                            label,
                            selected,
                            display_pending,
                            &self.owner,
                            |owner| owner.display.value,
                            set_checked,
                            ClipboardView::save_display,
                        )
                    }),
                ),
            )
            .child(
                settings_field_help(
                    tr(language, "分类筛选标签", "Category filter tabs"),
                    tr(
                        language,
                        "与主窗口工具栏分段一致；「全部」始终显示，且至少保留两个标签。关闭上方“显示分类筛选”时隐藏整条分段。",
                        "Matches the history toolbar tabs; “All” is always shown and at least two tabs stay visible. Turn off “Show category filters” above to hide the whole bar.",
                    ),
                    "category-tabs-help",
                )
                    .child(settings_inline().children(
                        category_tab_flags(&display.category_tabs)
                            .into_iter()
                            .zip(CATEGORY_TAB_SPECS)
                            .enumerate()
                            .map(|(index, (selected, (_, chinese, english, _)))| {
                                let owner = self.owner.clone();
                                let tabs_enabled =
                                    display.show_category_filter && !display_pending;
                                // 首项即「全部」：锁定开启，不受关闭影响。
                                let is_all_tab = index == 0;
                                let at_minimum_tabs =
                                    display.category_tabs.enabled_count() <= 2 && selected;
                                let checkbox_disabled =
                                    !tabs_enabled || is_all_tab || at_minimum_tabs;
                                settings_toggle(cx, Checkbox::new(("category-tab", index)))
                                    .label(tr(language, chinese, english))
                                    .checked(selected)
                                    .disabled(checkbox_disabled)
                                    .on_change(move |checked, _, cx| {
                                        let _ = owner.update(cx, |owner, cx| {
                                            let mut next = owner.display.value;
                                            set_category_tab_visible(
                                                &mut next.category_tabs,
                                                index,
                                                *checked,
                                            );
                                            // 开启分类栏时「全部」必须保留；disabled 已阻止
                                            // 关闭「全部」或缩减到两标签以下，故不再二次校验。
                                            if next.show_category_filter {
                                                next.category_tabs.all = true;
                                            }
                                            owner.save_display(next, cx);
                                        });
                                    })
                            }),
                    )),
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
                    |owner| owner.display.value.card_density,
                    |owner, density, cx| {
                        owner.save_display(
                            DisplayPreference {
                                card_density: density,
                                ..owner.display.value
                            },
                            cx,
                        );
                    },
                    cx,
                )),
            )
            .child(
                settings_field(tr(language, "卡片预览行数", "Card preview lines")).child(
                    NumberInput::new(self.preview_lines.input())
                        .w_full()
                        .h(px(visual::CONTROL_HEIGHT_LARGE))
                        .disabled(display_pending),
                ),
            )
            .child(
                settings_inline().children(
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
                        (
                            "show-card-number",
                            tr(language, "显示卡片序号", "Show card number"),
                            display.show_card_number,
                            |next: &mut DisplayPreference, checked| next.show_card_number = checked,
                        ),
                    ]
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        pref_checkbox(
                            cx,
                            id,
                            label,
                            selected,
                            display_pending,
                            &self.owner,
                            |owner| owner.display.value,
                            set_checked,
                            ClipboardView::save_display,
                        )
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
                    |owner| owner.display.value.time_format,
                    |owner, format, cx| {
                        owner.save_display(
                            DisplayPreference {
                                time_format: format,
                                ..owner.display.value
                            },
                            cx,
                        );
                    },
                    cx,
                )),
            )
            .child(
                settings_field(tr(language, "来源显示", "Source display")).child(
                    choice_tabs(
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
                        |owner| owner.display.value.source_app_display,
                        |owner, mode, cx| {
                            owner.save_display(
                                DisplayPreference {
                                    source_app_display: mode,
                                    ..owner.display.value
                                },
                                cx,
                            );
                        },
                        cx,
                    ),
                ),
            )
            .into_any_element()
    }
}
