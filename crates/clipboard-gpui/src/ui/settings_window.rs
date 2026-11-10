use super::*;

impl ClipboardView {
    pub(super) fn open_settings_window(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview("settings_opening", cx);
        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| {
                    native_window::set_window_visible(window, true)
                })
                .is_ok()
            {
                return;
            }
            self.settings_window = None;
        }
        if self.settings_window_opening {
            return;
        }

        self.settings_window_opening = true;
        self.refresh_data_size(cx);
        self.refresh_daily_counts(cx);
        cx.notify();
        let owner = cx.weak_entity();
        // Run after this view's update releases its borrow; unlike a spawned task,
        // a deferred effect also runs when the main window is hidden in the tray.
        cx.defer(move |cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let (theme, language) =
                owner_entity.update(cx, |owner, _| (owner.theme, owner.language));
            let window_effect = owner_entity.read(cx).window_effect;
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(800.), px(540.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some(tr(language, "设置", "Settings").into()),
                    ..TitleBar::title_bar_options()
                }),
                window_min_size: Some(size(px(700.), px(420.))),
                focus: true,
                show: true,
                app_id: Some("com.aslant.elegant-clipboard-gpui.settings".into()),
                ..TitleBar::window_options()
            };
            let settings_owner = owner.clone();
            let close_owner = owner.clone();
            let opened = gpui_kit::open_window(options, cx, move |window, cx| {
                apply_theme(theme, window, cx);
                acrylic::init_window_effect(window_effect, window, cx);
                window.set_window_title(tr(language, "设置", "Settings"));
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = close_owner.update(cx, |owner, cx| {
                        owner.settings_window = None;
                        cx.notify();
                    });
                    true
                });
                cx.new(|cx| SettingsWindowView::new(settings_owner.clone(), page, window, cx))
            });
            owner_entity.update(cx, |owner, cx| {
                owner.settings_window_opening = false;
                match opened {
                    Ok((handle, _)) => {
                        owner.settings_window = Some(handle);
                    }
                    Err(error) => {
                        owner.set_feedback_message(format!(
                            "{}: {error:?}",
                            tr(
                                owner.language,
                                "无法打开设置窗口",
                                "Unable to open settings"
                            )
                        ));
                        owner.is_error = true;
                    }
                }
                cx.notify();
            });
        });
    }
}

impl SettingsWindowView {
    pub(super) fn new(
        owner: WeakEntity<ClipboardView>,
        page: SettingsPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let entity = owner
            .upgrade()
            .expect("settings window requires its owner view");
        let language = entity.read(cx).language;
        let subscription = cx.observe_in(&entity, window, |this, owner, window, cx| {
            let owner = owner.read(cx);
            let fonts = owner.fonts.value.clone();
            let language = owner.language;
            let language_changed = language != this.language;
            this.language = language;
            this.sync_font_selects(&fonts, language, language_changed, window, cx);
            cx.notify();
        });
        let release_subscription = cx.observe_release(&entity, |this, owner, cx| {
            this.language = owner.language;
            cx.notify();
        });
        let app_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("notepad.exe or *browser*"));
        let capture_format_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("CF_UNICODETEXT"));
        let app_filter_input_subscription =
            cx.subscribe_in(&app_filter_input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.add_app_filter_rule(window, cx);
                }
            });
        let capture_format_input_subscription = cx.subscribe_in(
            &capture_format_input,
            window,
            |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.add_capture_format(window, cx);
                }
            },
        );
        let retention = entity.read(cx).retention.value;
        let retention_days_input =
            cx.new(|cx| InputState::new(window, cx).default_value(retention.days.to_string()));
        let retention_count_input =
            cx.new(|cx| InputState::new(window, cx).default_value(retention.max_count.to_string()));
        let effect = entity.read(cx).window_effect;
        let hover = entity.read(cx).hover_preference;
        let window_opacity = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(effect.opacity),
            NumberSpec {
                min: i32::from(MIN_WINDOW_EFFECT_OPACITY),
                max: i32::from(MAX_WINDOW_EFFECT_OPACITY),
                saved: |owner| {
                    (
                        i32::from(owner.window_effect.opacity),
                        owner.window_effect_pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_window_effect(
                        WindowEffectPreference {
                            opacity: value as u8,
                            ..owner.window_effect
                        },
                        cx,
                    );
                },
            },
        );
        let hover_delay = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(hover.delay_ms),
            NumberSpec {
                min: i32::from(MIN_HOVER_DELAY_MS),
                max: i32::from(MAX_HOVER_DELAY_MS),
                saved: |owner| {
                    (
                        i32::from(owner.hover_preference.delay_ms),
                        owner.hover_preference_pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_hover_preference(
                        HoverPreviewPreference {
                            delay_ms: value as u16,
                            ..owner.hover_preference
                        },
                        cx,
                    );
                },
            },
        );
        let hover_zoom = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(hover.zoom_step),
            NumberSpec {
                min: i32::from(MIN_HOVER_ZOOM_STEP),
                max: i32::from(MAX_HOVER_ZOOM_STEP),
                saved: |owner| {
                    (
                        i32::from(owner.hover_preference.zoom_step),
                        owner.hover_preference_pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_hover_preference(
                        HoverPreviewPreference {
                            zoom_step: value as u8,
                            ..owner.hover_preference
                        },
                        cx,
                    );
                },
            },
        );
        let preview_lines_last_value = entity.read(cx).display.card_max_lines;
        let preview_lines_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(preview_lines_last_value.to_string())
        });
        // Disable the input's default text-changing step so button presses emit Step events;
        // our save handler then applies bounds and the same pending/ACK policy as typed edits.
        preview_lines_input.update(cx, |input, cx| input.set_step(None, window, cx));
        let preview_lines_input_subscription = cx.subscribe_in(
            &preview_lines_input,
            window,
            |this, input, event, window, cx| {
                if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    return;
                }
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let (saved, pending) = {
                    let owner = owner.read(cx);
                    (owner.display.card_max_lines, owner.display_pending)
                };
                if pending {
                    return;
                }
                let value = input.read(cx).value();
                let parsed = value.parse::<i32>().ok();
                let lines = parsed
                    .map(|value| value.clamp(1, 10) as u8)
                    .unwrap_or(saved);
                if parsed != Some(i32::from(lines)) {
                    input.update(cx, |input, cx| {
                        input.set_value(lines.to_string(), window, cx)
                    });
                }
                this.save_preview_lines(lines, cx);
            },
        );
        let fonts = entity.read(cx).fonts.value.clone();
        let (ui_font_size, card_font_size, preview_font_size) =
            Self::bind_font_size_inputs(window, cx, fonts.clone());
        let font_default_label = tr(language, "系统默认", "System default");
        let (ui_font_select, ui_font_select_subscription) = Self::bind_font_select(
            &owner,
            &fonts,
            font_default_label,
            settings_fonts::FontPickerTarget::Ui,
            window,
            cx,
        );
        let (card_font_select, card_font_select_subscription) = Self::bind_font_select(
            &owner,
            &fonts,
            font_default_label,
            settings_fonts::FontPickerTarget::Card,
            window,
            cx,
        );
        let (preview_font_select, preview_font_select_subscription) = Self::bind_font_select(
            &owner,
            &fonts,
            font_default_label,
            settings_fonts::FontPickerTarget::Preview,
            window,
            cx,
        );
        let preview_lines_step_subscription = cx.subscribe_in(
            &preview_lines_input,
            window,
            |this, input, event: &NumberInputEvent, window, cx| {
                let NumberInputEvent::Step(action) = event;
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                let (saved, pending) = {
                    let owner = owner.read(cx);
                    (owner.display.card_max_lines, owner.display_pending)
                };
                if pending {
                    return;
                }
                let value = input.read(cx).value();
                let value = value.parse::<i32>().unwrap_or(i32::from(saved));
                let lines = match action {
                    StepAction::Increment => value.saturating_add(1),
                    StepAction::Decrement => value.saturating_sub(1),
                }
                .clamp(1, 10) as u8;
                if value != i32::from(lines) {
                    input.update(cx, |input, cx| {
                        input.set_value(lines.to_string(), window, cx)
                    });
                }
                this.save_preview_lines(lines, cx);
            },
        );
        Self {
            owner,
            _owner_subscription: subscription,
            _owner_release_subscription: release_subscription,
            language,
            page,
            shortcut_input: None,
            shortcut_editing: None,
            shortcut_capture_focus: cx.focus_handle(),
            shortcut_recording: false,
            shortcut_capture_error: None,
            recent_shortcuts_expanded: false,
            favorite_shortcuts_expanded: false,
            app_filter_input,
            _app_filter_input_subscription: app_filter_input_subscription,
            retention_days_input,
            retention_count_input,
            retention_error: None,
            preview_lines_input,
            _preview_lines_input_subscription: preview_lines_input_subscription,
            _preview_lines_step_subscription: preview_lines_step_subscription,
            preview_lines_last_value,
            preview_lines_last_pending: false,
            window_opacity,
            ui_font_size,
            card_font_size,
            preview_font_size,
            ui_font_select,
            card_font_select,
            preview_font_select,
            _font_select_subscriptions: (
                ui_font_select_subscription,
                card_font_select_subscription,
                preview_font_select_subscription,
            ),
            hover_delay,
            hover_zoom,
            app_picker_open: false,
            app_filter_error: false,
            capture_formats_expanded: false,
            capture_format_input,
            _capture_format_input_subscription: capture_format_input_subscription,
            capture_format_error: None,
            link_error: None,
        }
    }

    pub(super) fn save_preview_lines(&mut self, lines: u8, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        owner.update(cx, |owner, cx| {
            if !owner.display_pending && owner.display.card_max_lines != lines {
                owner.save_display(
                    DisplayPreference {
                        card_max_lines: lines,
                        ..owner.display
                    },
                    cx,
                );
            }
        });
    }

    pub(super) fn add_app_filter_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = self.app_filter_input.read(cx).value().trim().to_owned();
        if rule.is_empty() {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let accepted = owner.update(cx, |owner, cx| {
            let Some(next) = owner.app_filter.value.clone().with_rule(&rule) else {
                owner.set_feedback_message(
                    tr(
                        owner.language,
                        "规则无效或已达到上限",
                        "Invalid rule or rule limit reached",
                    )
                    .into(),
                );
                owner.is_error = true;
                cx.notify();
                return false;
            };
            if next == owner.app_filter.value {
                return true;
            }
            owner.save_app_filter(next, cx);
            owner.app_filter.pending
        });
        if accepted {
            self.app_filter_error = false;
            self.app_filter_input
                .update(cx, |input, cx| input.set_value(String::new(), window, cx));
            cx.notify();
        } else {
            self.app_filter_error = true;
            cx.notify();
        }
    }

    fn commit_bounded(
        &mut self,
        input: &Entity<InputState>,
        spec: NumberSpec,
        step: Option<StepAction>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let (current, pending) = (spec.saved)(owner.read(cx));
        if pending {
            return;
        }
        let parsed = input.read(cx).value().parse::<i32>().ok();
        let base = parsed.unwrap_or(current);
        let value = match step {
            Some(StepAction::Increment) => base.saturating_add(1),
            Some(StepAction::Decrement) => base.saturating_sub(1),
            None => base,
        }
        .clamp(spec.min, spec.max);
        if parsed != Some(value) || (step.is_some() && base != value) {
            input.update(cx, |input, cx| {
                input.set_value(value.to_string(), window, cx)
            });
        }
        owner.update(cx, |owner, cx| (spec.commit)(owner, value, cx));
    }
}

#[derive(Clone, Copy)]
pub(super) struct NumberSpec {
    min: i32,
    max: i32,
    saved: fn(&ClipboardView) -> (i32, bool),
    commit: fn(&mut ClipboardView, i32, &mut Context<ClipboardView>),
}

pub(super) struct BoundedNumberInput {
    input: Entity<InputState>,
    _edits: Subscription,
    _steps: Subscription,
    last_value: i32,
    last_pending: bool,
}

impl BoundedNumberInput {
    pub(super) fn bind(
        window: &mut Window,
        cx: &mut Context<SettingsWindowView>,
        initial: i32,
        spec: NumberSpec,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(initial.to_string()));
        // Button presses emit Step events; typed values commit on blur or Enter.
        input.update(cx, |input, cx| input.set_step(None, window, cx));
        let edits = cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
            if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                return;
            }
            this.commit_bounded(input, spec, None, window, cx);
        });
        let steps = cx.subscribe_in(
            &input,
            window,
            move |this, input, event: &NumberInputEvent, window, cx| {
                let NumberInputEvent::Step(action) = event;
                this.commit_bounded(input, spec, Some(*action), window, cx);
            },
        );
        Self {
            input,
            _edits: edits,
            _steps: steps,
            last_value: initial,
            last_pending: false,
        }
    }

    pub(super) fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    pub(super) fn sync(
        &mut self,
        confirmed: i32,
        pending: bool,
        window: &mut Window,
        cx: &mut Context<SettingsWindowView>,
    ) {
        let refresh = confirmed != self.last_value || (self.last_pending && !pending);
        self.last_value = confirmed;
        self.last_pending = pending;
        if !refresh {
            return;
        }
        let text = confirmed.to_string();
        if self.input.read(cx).value() != text {
            self.input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }
}

impl SettingsWindowView {
    pub(super) fn bind_font_size_inputs(
        window: &mut Window,
        cx: &mut Context<Self>,
        fonts: FontPreference,
    ) -> (BoundedNumberInput, BoundedNumberInput, BoundedNumberInput) {
        use clipboard_core::preferences::{
            MAX_CARD_FONT_SIZE, MAX_PREVIEW_FONT_SIZE, MAX_UI_FONT_SIZE, MIN_CARD_FONT_SIZE,
            MIN_PREVIEW_FONT_SIZE, MIN_UI_FONT_SIZE,
        };
        let ui_font_size = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(fonts.ui_font_size),
            NumberSpec {
                min: i32::from(MIN_UI_FONT_SIZE),
                max: i32::from(MAX_UI_FONT_SIZE),
                saved: |owner| {
                    (
                        i32::from(owner.fonts.value.ui_font_size),
                        owner.fonts.pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_fonts(
                        FontPreference {
                            ui_font_size: value as u8,
                            ..owner.fonts.value.clone()
                        },
                        cx,
                    );
                },
            },
        );
        let card_font_size = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(fonts.card_font_size),
            NumberSpec {
                min: i32::from(MIN_CARD_FONT_SIZE),
                max: i32::from(MAX_CARD_FONT_SIZE),
                saved: |owner| {
                    (
                        i32::from(owner.fonts.value.card_font_size),
                        owner.fonts.pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_fonts(
                        FontPreference {
                            card_font_size: value as u8,
                            ..owner.fonts.value.clone()
                        },
                        cx,
                    );
                },
            },
        );
        let preview_font_size = BoundedNumberInput::bind(
            window,
            cx,
            i32::from(fonts.preview_font_size),
            NumberSpec {
                min: i32::from(MIN_PREVIEW_FONT_SIZE),
                max: i32::from(MAX_PREVIEW_FONT_SIZE),
                saved: |owner| {
                    (
                        i32::from(owner.fonts.value.preview_font_size),
                        owner.fonts.pending,
                    )
                },
                commit: |owner, value, cx| {
                    owner.save_fonts(
                        FontPreference {
                            preview_font_size: value as u8,
                            ..owner.fonts.value.clone()
                        },
                        cx,
                    );
                },
            },
        );
        (ui_font_size, card_font_size, preview_font_size)
    }
}
