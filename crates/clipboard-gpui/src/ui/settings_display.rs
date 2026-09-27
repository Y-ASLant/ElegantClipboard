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
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().flex().gap_2().children(
                    [
                        (
                            "settings-window-language-zh",
                            "简体中文",
                            LanguagePreference::Chinese,
                        ),
                        (
                            "settings-window-language-en",
                            "English",
                            LanguagePreference::English,
                        ),
                    ]
                    .map(|(id, label, preference)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .outline()
                            .small()
                            .label(label)
                            .selected(language == preference)
                            .disabled(language_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    if owner.language != preference
                                        && owner.send(Command::SetLanguage(preference), cx)
                                    {
                                        owner.language_pending = true;
                                        cx.notify();
                                    }
                                });
                            })
                    }),
                ),
            )
            .child(
                div().flex().gap_2().children(
                    [
                        (
                            "settings-window-theme-system",
                            tr(language, "跟随系统", "System"),
                            IconName::LayoutDashboard,
                            ThemePreference::System,
                        ),
                        (
                            "settings-window-theme-light",
                            tr(language, "浅色", "Light"),
                            IconName::Sun,
                            ThemePreference::Light,
                        ),
                        (
                            "settings-window-theme-dark",
                            tr(language, "深色", "Dark"),
                            IconName::Moon,
                            ThemePreference::Dark,
                        ),
                    ]
                    .map(|(id, label, icon, preference)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .outline()
                            .small()
                            .icon(icon)
                            .tooltip(label)
                            .accessibility_label(label)
                            .selected(theme == preference)
                            .disabled(theme_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    if owner.theme != preference
                                        && owner.send(Command::SetTheme(preference), cx)
                                    {
                                        owner.theme_pending = true;
                                        cx.notify();
                                    }
                                });
                            })
                    }),
                ),
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
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div().flex().flex_wrap().gap_2().children(
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
            .child(
                div().flex().flex_wrap().gap_2().children(
                    [
                        (250, "250 ms"),
                        (500, "500 ms"),
                        (750, "750 ms"),
                        (1000, "1000 ms"),
                    ]
                    .map(|(delay, label)| {
                        let owner = self.owner.clone();
                        Button::new(("hover-delay", usize::from(delay)))
                            .outline()
                            .small()
                            .label(label)
                            .selected(hover_preference.delay_ms == delay)
                            .disabled(hover_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.save_hover_preference(
                                        HoverPreviewPreference {
                                            delay_ms: delay,
                                            ..hover_preference
                                        },
                                        cx,
                                    );
                                });
                            })
                    }),
                ),
            )
            .child(div().text_xs().child(tr(language, "浮窗位置", "Position")))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    [
                        (
                            "hover-auto",
                            tr(language, "自动", "Auto"),
                            HoverPreviewPosition::Auto,
                        ),
                        (
                            "hover-left",
                            tr(language, "左侧", "Left"),
                            HoverPreviewPosition::Left,
                        ),
                        (
                            "hover-right",
                            tr(language, "右侧", "Right"),
                            HoverPreviewPosition::Right,
                        ),
                    ]
                    .map(|(id, label, position)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .outline()
                            .small()
                            .label(label)
                            .selected(hover_preference.position == position)
                            .disabled(hover_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.save_hover_preference(
                                        HoverPreviewPreference {
                                            position,
                                            ..hover_preference
                                        },
                                        cx,
                                    );
                                });
                            })
                    }),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .child(tr(language, "图片缩放步进", "Image zoom step")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children([5, 10, 20, 50].map(|zoom_step| {
                        let owner = self.owner.clone();
                        Button::new(("hover-zoom-step", usize::from(zoom_step)))
                            .outline()
                            .small()
                            .label(format!("{zoom_step}%"))
                            .selected(hover_preference.zoom_step == zoom_step)
                            .disabled(hover_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.save_hover_preference(
                                        HoverPreviewPreference {
                                            zoom_step,
                                            ..hover_preference
                                        },
                                        cx,
                                    );
                                });
                            })
                    })),
            )
            .into_any_element()
    }

    pub(super) fn settings_audio_content(&self, copy: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let audio = owner_state.audio;
        let pending = owner_state.audio_pending;
        let kind = if copy { "copy" } else { "paste" };
        let enabled = if copy {
            audio.copy_enabled
        } else {
            audio.paste_enabled
        };
        let timing = if copy {
            audio.copy_timing
        } else {
            audio.paste_timing
        };
        let toggle_owner = self.owner.clone();
        let immediate_owner = self.owner.clone();
        let success_owner = self.owner.clone();
        let preview_owner = self.owner.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Checkbox::new(format!("audio-{kind}-enabled"))
                    .text_sm()
                    .label(tr(language, "启用", "Enable"))
                    .checked(enabled)
                    .disabled(pending)
                    .on_change(move |checked, _, cx| {
                        let _ = toggle_owner.update(cx, |owner, cx| {
                            let mut next = owner.audio;
                            if copy {
                                next.copy_enabled = *checked;
                            } else {
                                next.paste_enabled = *checked;
                            }
                            owner.save_audio(next, cx);
                        });
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "播放时机", "Timing"))
                    .child(
                        Button::new(format!("audio-{kind}-immediate"))
                            .outline()
                            .small()
                            .label(tr(language, "立即", "Immediate"))
                            .selected(timing == SoundTiming::Immediate)
                            .disabled(pending || !enabled)
                            .on_click(move |_, _, cx| {
                                let _ = immediate_owner.update(cx, |owner, cx| {
                                    let mut next = owner.audio;
                                    if copy {
                                        next.copy_timing = SoundTiming::Immediate;
                                    } else {
                                        next.paste_timing = SoundTiming::Immediate;
                                    }
                                    owner.save_audio(next, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("audio-{kind}-success"))
                            .outline()
                            .small()
                            .label(tr(language, "成功后", "After success"))
                            .selected(timing == SoundTiming::AfterSuccess)
                            .disabled(pending || !enabled)
                            .on_click(move |_, _, cx| {
                                let _ = success_owner.update(cx, |owner, cx| {
                                    let mut next = owner.audio;
                                    if copy {
                                        next.copy_timing = SoundTiming::AfterSuccess;
                                    } else {
                                        next.paste_timing = SoundTiming::AfterSuccess;
                                    }
                                    owner.save_audio(next, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("audio-{kind}-preview"))
                            .outline()
                            .small()
                            .label(tr(language, "试听", "Preview"))
                            .on_click(move |_, _, cx| {
                                let played =
                                    sound::play(if copy { Sound::Copy } else { Sound::Paste });
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
                    ),
            )
            .into_any_element()
    }

    pub(super) fn settings_toolbar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let toolbar = owner_state.toolbar;
        let toolbar_pending = owner_state.toolbar_pending;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(toolbar.items.into_iter().enumerate().map(|(index, item)| {
                let button = item.button;
                let label = match button {
                    ToolbarButton::Clear => tr(language, "清理历史", "Clear history"),
                    ToolbarButton::Batch => tr(language, "批量选择", "Batch select"),
                    ToolbarButton::Pin => tr(language, "置顶窗口", "Pin window"),
                    ToolbarButton::Settings => tr(language, "设置", "Settings"),
                };
                let (previous, next) = if button == ToolbarButton::Batch {
                    (None, None)
                } else {
                    (
                        toolbar.items[..index]
                            .iter()
                            .rev()
                            .find(|item| item.button != ToolbarButton::Batch)
                            .map(|item| item.button),
                        toolbar.items[index + 1..]
                            .iter()
                            .find(|item| item.button != ToolbarButton::Batch)
                            .map(|item| item.button),
                    )
                };
                let toggle_owner = self.owner.clone();
                let up_owner = self.owner.clone();
                let down_owner = self.owner.clone();
                let drag = ToolbarDrag {
                    button,
                    label,
                    toolbar,
                };
                let drag_entity = cx.entity();
                let target = cx
                    .has_active_drag()
                    .then_some(self.toolbar_drop_target)
                    .flatten()
                    .filter(|(target, _)| *target == button);
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap_2()
                    .on_drag_move(cx.listener(
                        move |this, event: &DragMoveEvent<ToolbarDrag>, _, cx| {
                            if !event.bounds.contains(&event.event.position) {
                                if this
                                    .toolbar_drop_target
                                    .is_some_and(|(target, _)| target == button)
                                {
                                    this.toolbar_drop_target = None;
                                    cx.notify();
                                }
                                return;
                            }
                            let drag = event.drag(cx);
                            let valid = this.owner.upgrade().is_some_and(|owner| {
                                let owner = owner.read(cx);
                                !owner.toolbar_pending && owner.toolbar == drag.toolbar
                            });
                            let next = (valid
                                && drag.button != button
                                && drag.button != ToolbarButton::Batch
                                && button != ToolbarButton::Batch
                                && item.visible)
                                .then_some((
                                    button,
                                    event.event.position.y > event.bounds.center().y,
                                ));
                            if this.toolbar_drop_target != next {
                                this.toolbar_drop_target = next;
                                cx.notify();
                            }
                        },
                    ))
                    .on_drop(cx.listener(move |this, drag: &ToolbarDrag, _, cx| {
                        let target = this.toolbar_drop_target.take();
                        let Some((target_button, after)) =
                            target.filter(|(target_button, _)| *target_button == button)
                        else {
                            cx.notify();
                            return;
                        };
                        if let Some(owner) = this.owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                if !owner.toolbar_pending
                                    && owner.toolbar == drag.toolbar
                                    && let Some(next) = drag.toolbar.move_before_or_after(
                                        drag.button,
                                        target_button,
                                        after,
                                    )
                                {
                                    owner.save_toolbar(next, cx);
                                }
                            });
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id(("toolbar-drag-handle", index))
                            .w(px(22.))
                            .h(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(cx.theme().muted_foreground)
                            .when(button != ToolbarButton::Batch, |handle| handle.child("⠿"))
                            .when(
                                item.visible && button != ToolbarButton::Batch && !toolbar_pending,
                                |handle| {
                                    handle.cursor_grab().on_drag(
                                        drag.clone(),
                                        move |drag, _, _, cx| {
                                            drag_entity.update(cx, |this, cx| {
                                                this.toolbar_drop_target = None;
                                                cx.notify();
                                            });
                                            cx.new(|_| drag.clone())
                                        },
                                    )
                                },
                            ),
                    )
                    .child(
                        Checkbox::new(("toolbar-visible", index))
                            .text_sm()
                            .label(label)
                            .checked(item.visible)
                            .disabled(toolbar_pending || button == ToolbarButton::Settings)
                            .on_change(move |checked, _, cx| {
                                if let Some(next) = toolbar.with_visibility(button, *checked) {
                                    let _ = toggle_owner.update(cx, |owner, cx| {
                                        owner.save_toolbar(next, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if button == ToolbarButton::Batch {
                                tr(language, "搜索框旁", "Beside search")
                            } else {
                                tr(language, "托盘菜单", "Tray menu")
                            }),
                    )
                    .child(
                        Button::new(("toolbar-up", index))
                            .small()
                            .ghost()
                            .label(tr(language, "上移", "Up"))
                            .disabled(toolbar_pending || previous.is_none())
                            .on_click(move |_, _, cx| {
                                if let Some(target) = previous
                                    && let Some(next) =
                                        toolbar.move_before_or_after(button, target, false)
                                {
                                    let _ = up_owner.update(cx, |owner, cx| {
                                        owner.save_toolbar(next, cx);
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new(("toolbar-down", index))
                            .small()
                            .ghost()
                            .label(tr(language, "下移", "Down"))
                            .disabled(toolbar_pending || next.is_none())
                            .on_click(move |_, _, cx| {
                                if let Some(target) = next
                                    && let Some(next) =
                                        toolbar.move_before_or_after(button, target, true)
                                {
                                    let _ = down_owner.update(cx, |owner, cx| {
                                        owner.save_toolbar(next, cx);
                                    });
                                }
                            }),
                    )
                    .when_some(target, |row, (_, after)| {
                        row.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .h(px(2.))
                                .bg(cx.theme().primary)
                                .when(after, |line| line.bottom_0())
                                .when(!after, |line| line.top_0()),
                        )
                    })
            }))
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
                div().flex().flex_wrap().gap_2().children(
                    [
                        (CardDensity::Compact, tr(language, "紧凑", "Compact")),
                        (CardDensity::Standard, tr(language, "标准", "Standard")),
                        (CardDensity::Spacious, tr(language, "宽松", "Spacious")),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (density, label))| {
                        let owner = self.owner.clone();
                        Button::new(("card-density", index))
                            .small()
                            .outline()
                            .label(label)
                            .selected(display.card_density == density)
                            .disabled(display_pending)
                            .on_click(move |_, _, cx| {
                                let next = DisplayPreference {
                                    card_density: density,
                                    ..display
                                };
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.save_display(next, cx);
                                });
                            })
                    }),
                ),
            )
            .child(
                div()
                    .flex()
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
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "时间格式", "Time format"))
                    .children(
                        [
                            (TimeFormat::Absolute, tr(language, "绝对时间", "Absolute")),
                            (TimeFormat::Relative, tr(language, "相对时间", "Relative")),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (format, label))| {
                            let owner = self.owner.clone();
                            Button::new(("time-format", index))
                                .small()
                                .outline()
                                .label(label)
                                .selected(display.time_format == format)
                                .disabled(display_pending || !display.show_time)
                                .on_click(move |_, _, cx| {
                                    let next = DisplayPreference {
                                        time_format: format,
                                        ..display
                                    };
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                })
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tr(language, "来源显示", "Source display"))
                    .children(
                        [
                            (
                                SourceAppDisplay::Both,
                                tr(language, "名称和图标", "Name and icon"),
                            ),
                            (SourceAppDisplay::Name, tr(language, "仅名称", "Name only")),
                            (SourceAppDisplay::Icon, tr(language, "仅图标", "Icon only")),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (mode, label))| {
                            let owner = self.owner.clone();
                            Button::new(("source-app-display", index))
                                .small()
                                .outline()
                                .label(label)
                                .selected(display.source_app_display == mode)
                                .disabled(display_pending || !display.show_source_app)
                                .on_click(move |_, _, cx| {
                                    let next = DisplayPreference {
                                        source_app_display: mode,
                                        ..display
                                    };
                                    let _ = owner.update(cx, |owner, cx| {
                                        owner.save_display(next, cx);
                                    });
                                })
                        }),
                    ),
            )
            .into_any_element()
    }
}
