use super::*;

impl SettingsWindowView {
    pub(super) fn begin_shortcut_edit(
        &mut self,
        target: ShortcutTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        let shortcut = match target {
            ShortcutTarget::Main => match &state.hotkey_choice {
                HotkeyPreference::Shortcut(value) => value.clone(),
                HotkeyPreference::Disabled => String::new(),
            },
            ShortcutTarget::Paste(favorite, slot) => state
                .paste_shortcuts
                .slot(favorite, slot)
                .unwrap_or_default()
                .to_owned(),
        };
        let shortcut_input = self.shortcut_input.get_or_insert_with(|| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Ctrl+Alt+1"))
        });
        shortcut_input.update(cx, |input, cx| input.set_value(shortcut, window, cx));
        self.shortcut_editing = Some(target);
        self.shortcut_recording = false;
        self.shortcut_capture_error = None;
        shortcut_input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn save_shortcut_edit(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.shortcut_editing else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(shortcut_input) = &self.shortcut_input else {
            return;
        };
        let shortcut = shortcut_input.read(cx).value().to_string();
        let accepted = match target {
            ShortcutTarget::Main => {
                let normalized = clipboard_platform::hotkey::normalize_main_shortcut(&shortcut);
                match normalized {
                    Ok(value) => owner.update(cx, |owner, cx| {
                        owner.select_hotkey(HotkeyPreference::Shortcut(value), cx);
                        owner.hotkey_pending
                    }),
                    Err(error) => {
                        self.shortcut_capture_error = Some(error.to_string());
                        cx.notify();
                        false
                    }
                }
            }
            ShortcutTarget::Paste(favorite, slot) => owner.update(cx, |owner, cx| {
                owner.save_paste_shortcut(favorite, slot, &shortcut, cx);
                owner.paste_shortcuts_pending.is_some()
            }),
        };
        if accepted {
            self.shortcut_editing = None;
            self.shortcut_recording = false;
            cx.notify();
        }
    }

    pub(super) fn start_shortcut_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shortcut_recording = true;
        self.shortcut_capture_error = None;
        self.shortcut_capture_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn capture_shortcut_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.shortcut_recording || event.is_held {
            return;
        }
        if event.keystroke.key.eq_ignore_ascii_case("escape")
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
            && !event.keystroke.modifiers.platform
        {
            self.shortcut_recording = false;
            self.shortcut_capture_error = None;
            cx.notify();
            return;
        }
        let shift_down = event.keystroke.modifiers.shift || input::shift_pressed();
        match shortcut_from_keystroke(
            &event.keystroke,
            shift_down,
            self.shortcut_editing == Some(ShortcutTarget::Main),
        ) {
            Ok(shortcut) => {
                if let Some(input) = &self.shortcut_input {
                    input.update(cx, |input, cx| input.set_value(shortcut, window, cx));
                }
                self.shortcut_recording = false;
                self.shortcut_capture_error = None;
            }
            Err(error) => self.shortcut_capture_error = Some(error),
        }
        cx.notify();
    }

    pub(super) fn shortcut_editor(
        &self,
        language: LanguagePreference,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1()
                    .child(tr(
                        language,
                        "输入组合键，例如",
                        "Enter a shortcut, for example",
                    ))
                    .child(shortcut_kbd("Ctrl+Alt+Z").expect("shortcut example")),
            )
            .child(Input::new(
                self.shortcut_input.as_ref().expect("editing input exists"),
            ))
            .when(self.shortcut_recording, |panel| {
                panel.child(
                    div()
                        .id("paste-shortcut-recorder")
                        .track_focus(&self.shortcut_capture_focus)
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().accent)
                        .p_2()
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_1()
                                .child(tr(language, "请按组合键；按", "Press a shortcut; press"))
                                .child(shortcut_kbd("Esc").expect("recording cancel key"))
                                .child(tr(language, "取消录制", "to cancel recording")),
                        )
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            this.capture_shortcut_key(event, window, cx);
                        })),
                )
            })
            .when_some(self.shortcut_capture_error.clone(), |panel, error| {
                panel.child(Alert::error("shortcut-capture-error", error).small())
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("paste-shortcut-record")
                            .small()
                            .outline()
                            .label(if self.shortcut_recording {
                                tr(language, "录制中", "Recording")
                            } else {
                                tr(language, "录制", "Record")
                            })
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.start_shortcut_recording(window, cx);
                            })),
                    )
                    .child(
                        Button::new("paste-shortcut-save")
                            .small()
                            .label(tr(language, "保存", "Save"))
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_shortcut_edit(cx);
                            })),
                    )
                    .child(
                        Button::new("paste-shortcut-cancel")
                            .small()
                            .ghost()
                            .label(tr(language, "取消", "Cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.shortcut_editing = None;
                                this.shortcut_recording = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn paste_slot_rows(
        &self,
        favorite: bool,
        shortcuts: &PasteShortcutConfig,
        pending: bool,
        language: LanguagePreference,
        cx: &mut Context<Self>,
    ) -> Div {
        let defaults = PasteShortcutConfig::default();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .children((1..=10u8).map(|slot| {
                let shortcut = shortcuts.slot(favorite, slot).unwrap_or_default();
                let shortcut_content = shortcut_kbd(shortcut).map_or_else(
                    || {
                        div()
                            .text_sm()
                            .child(if shortcut.is_empty() {
                                tr(language, "未设置", "Not set").to_owned()
                            } else {
                                shortcut.to_owned()
                            })
                            .into_any_element()
                    },
                    |kbd| kbd.into_any_element(),
                );
                let default = defaults.slot(favorite, slot).unwrap_or_default().to_owned();
                let owner_disable = self.owner.clone();
                let owner_reset = self.owner.clone();
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w(px(220.))
                            .gap_2()
                            .child(div().w(px(96.)).text_xs().child(format!(
                                "{} {slot}",
                                tr(
                                    language,
                                    if favorite { "收藏" } else { "普通" },
                                    if favorite { "Favorite" } else { "Recent" }
                                )
                            )))
                            .child(shortcut_content),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new(format!("paste-slot-edit-{favorite}-{slot}"))
                                    .outline()
                                    .small()
                                    .label(tr(language, "修改", "Edit"))
                                    .disabled(pending)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.begin_shortcut_edit(
                                            ShortcutTarget::Paste(favorite, slot),
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new(format!("paste-slot-disable-{favorite}-{slot}"))
                                    .ghost()
                                    .small()
                                    .label(tr(language, "停用", "Disable"))
                                    .disabled(pending || shortcut.is_empty())
                                    .on_click(move |_, _, cx| {
                                        let _ = owner_disable.update(cx, |owner, cx| {
                                            owner.save_paste_shortcut(favorite, slot, "", cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new(format!("paste-slot-reset-{favorite}-{slot}"))
                                    .ghost()
                                    .small()
                                    .label(tr(language, "默认", "Default"))
                                    .disabled(pending || shortcut == default)
                                    .on_click(move |_, _, cx| {
                                        let _ = owner_reset.update(cx, |owner, cx| {
                                            owner.save_paste_shortcut(favorite, slot, &default, cx);
                                        });
                                    }),
                            ),
                    )
                    .when(
                        self.shortcut_editing == Some(ShortcutTarget::Paste(favorite, slot)),
                        |row| row.child(self.shortcut_editor(language, pending, cx)),
                    )
            }))
    }

    pub(super) fn paste_group_actions(
        &self,
        favorite: bool,
        shortcuts: &PasteShortcutConfig,
        pending: bool,
        language: LanguagePreference,
    ) -> Div {
        let defaults = PasteShortcutConfig::default();
        let (current, defaults) = if favorite {
            (&shortcuts.favorites, &defaults.favorites)
        } else {
            (&shortcuts.recent, &defaults.recent)
        };
        let reset_owner = self.owner.clone();
        let disable_owner = self.owner.clone();
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new(format!("paste-{favorite}-reset-all"))
                    .outline()
                    .small()
                    .label(tr(language, "全部恢复默认", "Restore all defaults"))
                    .disabled(pending || current == defaults)
                    .on_click(move |_, _, cx| {
                        let _ = reset_owner.update(cx, |owner, cx| {
                            owner.set_paste_shortcut_group_defaults(favorite, true, cx);
                        });
                    }),
            )
            .child(
                Button::new(format!("paste-{favorite}-disable-all"))
                    .outline()
                    .small()
                    .label(tr(language, "全部停用", "Disable all"))
                    .disabled(pending || current.iter().all(String::is_empty))
                    .on_click(move |_, _, cx| {
                        let _ = disable_owner.update(cx, |owner, cx| {
                            owner.set_paste_shortcut_group_defaults(favorite, false, cx);
                        });
                    }),
            )
    }
}
