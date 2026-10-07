use super::settings_common::choice_tabs;
use super::*;
use gpui_kit::component::{checkbox::Checkbox, input::NumberInput, switch::Switch};

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
            .into_any_element()
    }

    pub(super) fn settings_hover_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let hover_preference = owner_state.hover_preference;
        let hover_pending = owner_state.hover_preference_pending;
        let expanded_hover_owner = self.owner.clone();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "预览内容", "Preview content"))
                    .children(
                        [
                            (
                                "hover-image",
                                tr(language, "图片", "Images"),
                                hover_preference.image,
                                (|next: &mut HoverPreviewPreference, checked| next.image = checked)
                                    as fn(&mut HoverPreviewPreference, bool),
                            ),
                            (
                                "hover-text",
                                tr(language, "文本", "Text"),
                                hover_preference.text,
                                |next: &mut HoverPreviewPreference, checked| next.text = checked,
                            ),
                            (
                                "hover-files",
                                tr(language, "文件", "Files"),
                                hover_preference.files,
                                |next: &mut HoverPreviewPreference, checked| next.files = checked,
                            ),
                        ]
                        .map(|(id, label, selected, set_checked)| {
                            let owner = self.owner.clone();
                            Checkbox::new(id)
                                .text_sm()
                                .label(label)
                                .checked(selected)
                                .disabled(hover_pending)
                                .on_change(move |checked, _, cx| {
                                    let _ = owner.update(cx, |owner, cx| {
                                        let mut next = owner.hover_preference;
                                        set_checked(&mut next, *checked);
                                        owner.save_hover_preference(next, cx);
                                    });
                                })
                        }),
                    ),
            )
            .child(
                Checkbox::new("hover-expanded-image")
                    .text_sm()
                    .label(tr(
                        language,
                        "大图使用更大浮窗",
                        "Expand image hover window",
                    ))
                    .checked(hover_preference.expanded_image)
                    .disabled(hover_pending || !hover_preference.image)
                    .on_change(move |checked, _, cx| {
                        let _ = expanded_hover_owner.update(cx, |owner, cx| {
                            owner.save_hover_preference(
                                HoverPreviewPreference {
                                    expanded_image: *checked,
                                    ..owner.hover_preference
                                },
                                cx,
                            );
                        });
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "停留延时", "Hover delay")),
            )
            .child(choice_tabs(
                "hover-delay",
                &self.owner,
                [250, 500, 750, 1000],
                ["250 ms", "500 ms", "750 ms", "1000 ms"],
                hover_preference.delay_ms,
                hover_pending,
                |owner| owner.hover_preference.delay_ms,
                |owner, delay, cx| {
                    owner.save_hover_preference(
                        HoverPreviewPreference {
                            delay_ms: delay,
                            ..owner.hover_preference
                        },
                        cx,
                    );
                },
            ))
            .child(div().text_xs().child(tr(language, "浮窗位置", "Position")))
            .child(choice_tabs(
                "hover-position",
                &self.owner,
                [
                    HoverPreviewPosition::Auto,
                    HoverPreviewPosition::Left,
                    HoverPreviewPosition::Right,
                ],
                [
                    tr(language, "自动", "Auto"),
                    tr(language, "左侧", "Left"),
                    tr(language, "右侧", "Right"),
                ],
                hover_preference.position,
                hover_pending,
                |owner| owner.hover_preference.position,
                |owner, position, cx| {
                    owner.save_hover_preference(
                        HoverPreviewPreference {
                            position,
                            ..owner.hover_preference
                        },
                        cx,
                    );
                },
            ))
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "图片缩放步进", "Image zoom step")),
            )
            .child(choice_tabs(
                "hover-zoom-step",
                &self.owner,
                [5, 10, 20, 50],
                ["5%", "10%", "20%", "50%"],
                hover_preference.zoom_step,
                hover_pending,
                |owner| owner.hover_preference.zoom_step,
                |owner, zoom_step, cx| {
                    owner.save_hover_preference(
                        HoverPreviewPreference {
                            zoom_step,
                            ..owner.hover_preference
                        },
                        cx,
                    );
                },
            ))
            .into_any_element()
    }

    pub(super) fn settings_audio_content(
        &self,
        channel: SoundChannel,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let audio = owner_state.audio;
        let pending = owner_state.audio_pending;
        let (switch_id, timing_id, preview_id) = channel.ids();
        let enabled = channel.enabled(audio);
        let timing = channel.timing(audio);
        let toggle_owner = self.owner.clone();
        let preview_owner = self.owner.clone();
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_xs().child(tr(language, "播放时机", "Timing")))
            .child(div().flex_1().min_w(px(0.)).child(choice_tabs(
                timing_id,
                &self.owner,
                [SoundTiming::Immediate, SoundTiming::AfterSuccess],
                [
                    tr(language, "立即", "Immediate"),
                    tr(language, "成功后", "After success"),
                ],
                timing,
                pending || !enabled,
                move |owner: &ClipboardView| channel.timing(owner.audio),
                move |owner, timing, cx| {
                    let mut next = owner.audio;
                    channel.set_timing(&mut next, timing);
                    owner.save_audio(next, cx);
                },
            )))
            .child(
                Button::new(preview_id)
                    .outline()
                    .small()
                    .label(tr(language, "试听", "Preview"))
                    .on_click(move |_, _, cx| {
                        let played = sound::play(channel.sound());
                        if !played {
                            let _ = preview_owner.update(cx, |owner, cx| {
                                owner.message = tr(
                                    owner.language,
                                    "音频设备不可用，无法试听",
                                    "Audio device unavailable; preview could not play",
                                )
                                .into();
                                owner.is_error = true;
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                Switch::new(switch_id)
                    .label(tr(language, "启用", "Enable"))
                    .checked(enabled)
                    .disabled(pending)
                    .on_change(move |checked, _, cx| {
                        let _ = toggle_owner.update(cx, |owner, cx| {
                            let mut next = owner.audio;
                            channel.set_enabled(&mut next, *checked);
                            owner.save_audio(next, cx);
                        });
                    }),
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
