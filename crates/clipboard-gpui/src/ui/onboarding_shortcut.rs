use super::super::*;

pub(in crate::ui) struct OnboardingShortcut {
    owner: WeakEntity<ClipboardView>,
    input: Entity<InputState>,
    capture_focus: FocusHandle,
    recording: bool,
    submitted: Option<String>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl OnboardingShortcut {
    pub(in crate::ui) fn new(
        owner: WeakEntity<ClipboardView>,
        initial: HotkeyPreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = match initial {
            HotkeyPreference::Shortcut(value) => value,
            HotkeyPreference::Disabled => String::new(),
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Ctrl+Alt+Z")
                .default_value(value)
        });
        let mut subscriptions = Vec::with_capacity(3);
        subscriptions.push(
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.save(window, cx);
                }
            }),
        );
        let recording = cx.weak_entity();
        let recording_window = window.window_handle().window_id();
        subscriptions.push(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle().window_id() != recording_window {
                return;
            }
            let _ = recording.update(cx, |this, cx| {
                if this.recording && this.capture_focus.is_focused(window) {
                    this.capture_key(&event.keystroke, window, cx);
                }
            });
        }));
        if let Some(entity) = owner.upgrade() {
            subscriptions.push(cx.observe_in(&entity, window, |this, owner, _, cx| {
                let owner = owner.read(cx);
                if !owner.hotkey_pending
                    && let Some(submitted) = this.submitted.take()
                {
                    let saved = matches!(
                        &owner.hotkey_choice,
                        HotkeyPreference::Shortcut(value) if value == &submitted
                    ) && owner.hotkey.is_some()
                        && !owner.is_error;
                    this.error = (!saved).then(|| owner.message.clone());
                }
                // The actual state is rendered from the owner; keep the editable draft intact.
                cx.notify();
            }));
        }
        Self {
            owner,
            input,
            capture_focus: cx.focus_handle(),
            recording: false,
            submitted: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    pub(in crate::ui) fn suspend(&mut self, cx: &mut Context<Self>) {
        self.recording = false;
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.onboarding_saving()
            || crate::capabilities::global_shortcut_unavailable_reason(state.language).is_some()
            || self.recording
        {
            return;
        }
        let normalized = clipboard_platform::hotkey::normalize_main_shortcut(
            self.input.read(cx).value().as_ref(),
        );
        let shortcut = match normalized {
            Ok(shortcut) => shortcut,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let unchanged = matches!(
            &state.hotkey_choice,
            HotkeyPreference::Shortcut(value) if value == &shortcut
        ) && state.hotkey.is_some();
        self.error = None;
        self.input.update(cx, |input, cx| {
            input.set_value(shortcut.clone(), window, cx);
        });
        if unchanged {
            cx.notify();
            return;
        }
        owner.update(cx, |owner, cx| {
            owner.select_hotkey(HotkeyPreference::Shortcut(shortcut.clone()), cx);
            if owner.hotkey_pending {
                self.submitted = Some(shortcut);
            } else if owner.is_error {
                self.error = Some(owner.message.clone());
            }
        });
        cx.notify();
    }

    fn start_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let owner = owner.read(cx);
        if owner.onboarding_saving()
            || crate::capabilities::global_shortcut_unavailable_reason(owner.language).is_some()
        {
            return;
        }
        self.recording = true;
        self.error = None;
        self.capture_focus.focus(window, cx);
        cx.notify();
    }

    fn capture_key(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        // Intercept before application bindings, so Escape cancels recording
        // rather than completing the guide and other bound chords can be recorded.
        cx.stop_propagation();
        let modifiers = keystroke.modifiers;
        if keystroke.key.eq_ignore_ascii_case("escape")
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && !modifiers.platform
        {
            self.recording = false;
            self.error = None;
            self.input.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return;
        }
        match shortcut_from_keystroke(keystroke, modifiers.shift || input::shift_pressed(), true) {
            Ok(shortcut) => {
                self.input.update(cx, |input, cx| {
                    input.set_value(shortcut, window, cx);
                    input.focus(window, cx);
                });
                self.recording = false;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }
}

impl Render for OnboardingShortcut {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div();
        };
        let owner = owner.read(cx);
        let language = owner.language;
        let registered = owner.hotkey.is_some();
        let disabled = owner.hotkey_choice == HotkeyPreference::Disabled;
        let pending = owner.shortcut_change_pending();
        let used = owner.onboarding_hotkey_used;
        let chord = match &owner.hotkey_choice {
            HotkeyPreference::Shortcut(value) => shortcut_kbd(value),
            HotkeyPreference::Disabled => None,
        }
        .map(|kbd| {
            kbd.text_size(px(visual::TEXT_BODY))
                .px_2()
                .py_1()
                .rounded(cx.theme().radius)
                .into_any_element()
        })
        .unwrap_or_else(|| {
            div()
                .text_sm()
                .font_semibold()
                .child(hotkey_label(language, &owner.hotkey_choice).to_owned())
                .into_any_element()
        });
        let unavailable = crate::capabilities::global_shortcut_unavailable_reason(language);
        let locked = owner.onboarding_saving() || unavailable.is_some();
        let status = if pending {
            tr(language, "正在保存快捷键…", "Saving shortcut…")
        } else if disabled {
            tr(language, "快捷键已关闭", "Shortcut is disabled")
        } else if registered {
            tr(
                language,
                "已注册，可在其他应用中使用",
                "Registered and available in other apps",
            )
        } else {
            tr(language, "快捷键未注册", "Shortcut is not registered")
        };
        div()
            .w_full()
            .min_w_0()
            .p(px(visual::SPACE_4))
            .rounded(cx.theme().radius_lg)
            .border_1()
            .border_color(crate::design_system::container_border(cx))
            .bg(cx.theme().group_box)
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_4))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_2))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(visual::SPACE_2))
                            .child(
                                div()
                                    .text_size(px(visual::TEXT_BODY))
                                    .font_medium()
                                    .child(tr(language, "当前快捷键", "Current shortcut")),
                            )
                            .child(chord),
                    )
                    .child(
                        div()
                            .text_size(px(visual::TEXT_SMALL))
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                    .child(
                        div()
                            .text_sm()
                            .line_height(relative(1.5))
                            .text_color(cx.theme().muted_foreground)
                            .child(if registered && !pending {
                                if used {
                                    tr(language, "已收到快捷键，随时可用它打开或隐藏面板。", "Shortcut received. Use it to show or hide this panel.")
                                } else {
                                    tr(language, "按一次隐藏面板，再按一次返回引导。", "Press once to hide the panel, then again to return.")
                                }
                            } else {
                                tr(language, "你仍可从系统托盘打开面板并继续引导。", "You can still open this panel from the system tray and continue the guide.")
                            }),
                    ),
            )
            .when(disabled && unavailable.is_none(), |panel| {
                panel.child(
                    Alert::warning(
                        "onboarding-hotkey-disabled",
                        tr(language, "输入或录制快捷键并保存即可启用；也可稍后在设置中配置。", "Enter or record a shortcut and save to enable it, or configure it later in Settings."),
                    )
                    .small(),
                )
            })
            .when_some(unavailable, |panel, reason| {
                panel.child(Alert::warning("onboarding-hotkey-platform", reason).small())
            })
            .when(!disabled && !registered && unavailable.is_none(), |panel| {
                panel.child(
                    Alert::warning(
                        "onboarding-hotkey-unregistered",
                        tr(language, "当前组合键无法使用。请尝试其他组合键，或再次保存以重试注册。", "The current shortcut is unavailable. Try another combination, or save again to retry registration."),
                    )
                    .small(),
                )
            })
            .child(
                div()
                    .pt(px(visual::SPACE_4))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_2))
                    .child(div().text_sm().font_medium().child(tr(
                        language,
                        "更换快捷键",
                        "Change shortcut",
                    )))
                    .child(
                        Input::new(&self.input)
                            .disabled(locked || self.recording)
                            .refine_style(&StyleRefinement::default().h(px(visual::CONTROL_HEIGHT_LARGE))),
                    ),
            )
            .when(self.recording, |panel| {
                panel.child(
                    div()
                        .id("onboarding-shortcut-recorder")
                        .track_focus(&self.capture_focus)
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().primary)
                        .p_3()
                        .text_sm()
                        .child(tr(language, "按下组合键；Esc 取消，保存后生效。", "Press a shortcut. Esc cancels; save to apply.")),
                )
            })
            .when_some(self.error.clone(), |panel, error| {
                panel.child(
                    Alert::error(
                        "onboarding-shortcut-error",
                        format!(
                            "{}: {error}",
                            tr(language, "快捷键未保存，请修改或重试", "Shortcut not saved; edit or retry")
                        ),
                    )
                    .small(),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("onboarding-shortcut-record")
                            .outline()
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(tr(language, "录制", "Record"))
                            .disabled(locked || self.recording)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_recording(window, cx);
                            })),
                    )
                    .when(self.recording, |buttons| {
                        buttons.child(
                            Button::new("onboarding-shortcut-cancel-recording")
                                .ghost()
                                .h(px(visual::CONTROL_HEIGHT_LARGE))
                                .label(tr(language, "取消录制", "Cancel recording"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.suspend(cx);
                                    this.input.update(cx, |input, cx| input.focus(window, cx));
                                })),
                        )
                    })
                    .child(
                        Button::new("onboarding-shortcut-save")
                            .primary()
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(if pending {
                                tr(language, "保存中…", "Saving…")
                            } else {
                                tr(language, "保存更改", "Save changes")
                            })
                            .disabled(locked || self.recording)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save(window, cx);
                            })),
                    ),
            )
    }
}
