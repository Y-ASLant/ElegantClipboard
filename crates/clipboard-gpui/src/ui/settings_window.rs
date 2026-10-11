use super::*;
impl SettingsPage {
    pub(super) const ALL: [Self; 8] = [
        Self::General,
        Self::Display,
        Self::Theme,
        Self::Data,
        Self::AppFilter,
        Self::Audio,
        Self::Shortcuts,
        Self::About,
    ];

    pub(super) fn label(self, language: LanguagePreference) -> &'static str {
        match self {
            Self::General => tr(language, "常规", "General"),
            Self::Display => tr(language, "显示", "Display"),
            Self::Theme => tr(language, "外观", "Appearance"),
            Self::Data => tr(language, "数据", "Data"),
            Self::AppFilter => tr(language, "应用过滤", "App filter"),
            Self::Audio => tr(language, "音效", "Audio"),
            Self::Shortcuts => tr(language, "快捷键", "Shortcuts"),
            Self::About => tr(language, "关于", "About"),
        }
    }

    pub(super) fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Display => IconName::Eye,
            Self::Theme => IconName::Palette,
            Self::Data => IconName::HardDrive,
            Self::AppFilter => IconName::Search,
            Self::Audio => IconName::Bell,
            Self::Shortcuts => IconName::SquareTerminal,
            Self::About => IconName::Info,
        }
    }
}

impl ClipboardView {
    pub(super) fn open_settings_window(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        if let Some(handle) = self.settings_window {
            if handle
                .update(cx, |_, window, _| tray::set_window_visible(window, true))
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
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(720.)),
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
        let subscription = cx.observe_in(&entity, window, |this, owner, _, cx| {
            let owner = owner.read(cx);
            this.language = owner.language;
            cx.notify();
        });
        let release_subscription = cx.observe_release(&entity, |this, owner, cx| {
            this.language = owner.language;
            cx.notify();
        });
        let app_filter_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("notepad.exe or *browser*"));
        let app_filter_input_subscription =
            cx.subscribe_in(&app_filter_input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.add_app_filter_rule(window, cx);
                }
            });
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
            preview_lines_input,
            _preview_lines_input_subscription: preview_lines_input_subscription,
            _preview_lines_step_subscription: preview_lines_step_subscription,
            preview_lines_last_value,
            preview_lines_last_pending: false,
            app_picker_open: false,
            app_filter_error: false,
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
}
