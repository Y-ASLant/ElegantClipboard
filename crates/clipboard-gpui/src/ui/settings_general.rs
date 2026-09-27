use super::*;

impl SettingsWindowView {
    pub(super) fn settings_shortcut_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (language, hotkey_choice, hotkey_pending) = {
            let state = owner_entity.read(cx);
            (state.language, state.hotkey_choice, state.hotkey_pending)
        };

        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(
                [
                    (
                        "settings-window-hotkey-ctrl-shift-v",
                        HotkeyPreference::CtrlShiftV,
                    ),
                    ("settings-window-hotkey-alt-c", HotkeyPreference::AltC),
                    (
                        "settings-window-hotkey-ctrl-alt-v",
                        HotkeyPreference::CtrlAltV,
                    ),
                    (
                        "settings-window-hotkey-disabled",
                        HotkeyPreference::Disabled,
                    ),
                ]
                .map(|(id, choice)| {
                    let owner = self.owner.clone();
                    let button = Button::new(id)
                        .outline()
                        .small()
                        .selected(hotkey_choice == choice)
                        .disabled(hotkey_pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                owner.select_hotkey(choice, cx);
                            });
                        });
                    if choice == HotkeyPreference::Disabled {
                        button.label(hotkey_label(language, choice))
                    } else {
                        button.child(shortcut_kbd(choice.label()).expect("built-in hotkey"))
                    }
                }),
            )
            .into_any_element()
    }

    pub(super) fn settings_positioning_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (language, window_position, window_position_pending) = {
            let state = owner_entity.read(cx);
            (
                state.language,
                state.window_position,
                state.window_position_pending,
            )
        };

        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(
                [
                    (
                        "window-position-cursor",
                        tr(language, "跟随光标", "Follow cursor"),
                        WindowPositionPreference::FollowCursor,
                    ),
                    (
                        "window-position-center",
                        tr(language, "当前屏幕居中", "Center on screen"),
                        WindowPositionPreference::ScreenCenter,
                    ),
                    (
                        "window-position-fixed",
                        tr(language, "保持位置", "Keep position"),
                        WindowPositionPreference::FixedPosition,
                    ),
                ]
                .map(|(id, label, preference)| {
                    let owner = self.owner.clone();
                    Button::new(id)
                        .outline()
                        .small()
                        .label(label)
                        .selected(window_position == preference)
                        .disabled(window_position_pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                if owner.window_position != preference
                                    && owner.send(Command::SetWindowPosition(preference), cx)
                                {
                                    owner.window_position_pending = true;
                                    cx.notify();
                                }
                            });
                        })
                }),
            )
            .into_any_element()
    }

    pub(super) fn settings_behavior_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (
            language,
            persist_window_size,
            persist_window_size_pending,
            auto_reset_state,
            auto_reset_state_pending,
            search_auto_focus,
            search_auto_focus_pending,
            search_auto_clear,
            search_auto_clear_pending,
            skip_clear_confirm,
            skip_clear_confirm_pending,
            paste_close_window,
            paste_close_window_pending,
            paste_move_to_top,
            paste_move_to_top_pending,
        ) = {
            let state = owner_entity.read(cx);
            (
                state.language,
                state.persist_window_size,
                state.persist_window_size_pending,
                state.auto_reset_state,
                state.auto_reset_state_pending,
                state.search_auto_focus,
                state.search_auto_focus_pending,
                state.search_auto_clear,
                state.search_auto_clear_pending,
                state.skip_clear_confirm,
                state.skip_clear_confirm_pending,
                state.paste_close_window,
                state.paste_close_window_pending,
                state.paste_move_to_top,
                state.paste_move_to_top_pending,
            )
        };
        let persist_owner = self.owner.clone();
        let reset_owner = self.owner.clone();
        let search_focus_owner = self.owner.clone();
        let search_clear_owner = self.owner.clone();
        let skip_clear_owner = self.owner.clone();
        let paste_close_owner = self.owner.clone();
        let paste_move_owner = self.owner.clone();

        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(
                Button::new("persist-window-size")
                    .outline()
                    .small()
                    .label(tr(language, "记住窗口大小", "Remember window size"))
                    .selected(persist_window_size)
                    .disabled(persist_window_size_pending)
                    .on_click(move |_, _, cx| {
                        let _ = persist_owner.update(cx, |owner, cx| {
                            if owner.send(
                                Command::SetPersistWindowSize(!owner.persist_window_size),
                                cx,
                            ) {
                                owner.persist_window_size_pending = true;
                                owner.window_size_task = None;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("auto-reset-state")
                    .outline()
                    .small()
                    .label(tr(
                        language,
                        "隐藏时重置搜索、筛选和滚动",
                        "Reset search, filters and scroll on hide",
                    ))
                    .selected(auto_reset_state)
                    .disabled(auto_reset_state_pending)
                    .on_click(move |_, _, cx| {
                        let _ = reset_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetAutoResetState(!owner.auto_reset_state), cx) {
                                owner.auto_reset_state_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("search-auto-focus")
                    .outline()
                    .small()
                    .label(tr(language, "唤出时聚焦搜索", "Focus search when shown"))
                    .selected(search_auto_focus)
                    .disabled(search_auto_focus_pending)
                    .on_click(move |_, _, cx| {
                        let _ = search_focus_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetSearchAutoFocus(!owner.search_auto_focus), cx)
                            {
                                owner.search_auto_focus_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("search-auto-clear")
                    .outline()
                    .small()
                    .label(tr(language, "唤出时清空搜索", "Clear search when shown"))
                    .selected(search_auto_clear)
                    .disabled(search_auto_clear_pending)
                    .on_click(move |_, _, cx| {
                        let _ = search_clear_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetSearchAutoClear(!owner.search_auto_clear), cx)
                            {
                                owner.search_auto_clear_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("skip-clear-confirm")
                    .outline()
                    .small()
                    .label(tr(
                        language,
                        "清理历史免确认",
                        "Clear history without confirmation",
                    ))
                    .selected(skip_clear_confirm)
                    .disabled(skip_clear_confirm_pending)
                    .on_click(move |_, _, cx| {
                        let _ = skip_clear_owner.update(cx, |owner, cx| {
                            if owner
                                .send(Command::SetSkipClearConfirm(!owner.skip_clear_confirm), cx)
                            {
                                owner.skip_clear_confirm_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("paste-close-window")
                    .outline()
                    .small()
                    .label(tr(language, "粘贴后关闭窗口", "Close after paste"))
                    .selected(paste_close_window)
                    .disabled(paste_close_window_pending)
                    .on_click(move |_, _, cx| {
                        let _ = paste_close_owner.update(cx, |owner, cx| {
                            if owner
                                .send(Command::SetPasteCloseWindow(!owner.paste_close_window), cx)
                            {
                                owner.paste_close_window_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Button::new("paste-move-to-top")
                    .outline()
                    .small()
                    .label(tr(
                        language,
                        "粘贴后移到列表首位",
                        "Move to top after paste",
                    ))
                    .selected(paste_move_to_top)
                    .disabled(paste_move_to_top_pending)
                    .on_click(move |_, _, cx| {
                        let _ = paste_move_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetPasteMoveToTop(!owner.paste_move_to_top), cx)
                            {
                                owner.paste_move_to_top_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .into_any_element()
    }

    pub(super) fn settings_startup_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (language, autostart, autostart_pending) = {
            let state = owner_entity.read(cx);
            (state.language, state.autostart, state.autostart_pending)
        };
        let startup_owner = self.owner.clone();
        div()
            .child(
                Button::new("settings-window-autostart")
                    .outline()
                    .small()
                    .icon(if autostart {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .label(if autostart {
                        tr(language, "已开启", "Enabled")
                    } else {
                        tr(language, "已关闭", "Disabled")
                    })
                    .selected(autostart)
                    .disabled(autostart_pending)
                    .on_click(move |_, _, cx| {
                        let _ = startup_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetAutostart(!owner.autostart), cx) {
                                owner.autostart_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .into_any_element()
    }

    pub(super) fn settings_quick_paste_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (
            language,
            quick_paste_enabled,
            quick_paste_pending_setting,
            paste_key,
            paste_key_pending,
            paste_shortcuts,
            paste_shortcuts_pending,
            shortcut_actions_pending,
            paste_shortcut_status,
            monitoring,
        ) = {
            let state = owner.read(cx);
            (
                state.language,
                state.quick_paste_enabled,
                state.quick_paste_pending_setting,
                state.paste_key,
                state.paste_key_pending,
                state.paste_shortcuts.clone(),
                state.paste_shortcuts_pending.is_some(),
                state.paste_shortcuts_pending.is_some()
                    || state.quick_paste_pending_setting
                    || state.hotkey_pending,
                state.paste_shortcut_status.clone(),
                state.monitoring,
            )
        };
        let quick_paste_owner = self.owner.clone();
        let recent_shortcut_rows = self.recent_shortcuts_expanded.then(|| {
            self.paste_slot_rows(
                false,
                &paste_shortcuts,
                shortcut_actions_pending,
                language,
                cx,
            )
            .into_any_element()
        });
        let favorite_shortcut_rows = self.favorite_shortcuts_expanded.then(|| {
            self.paste_slot_rows(
                true,
                &paste_shortcuts,
                shortcut_actions_pending,
                language,
                cx,
            )
            .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(
                Button::new("quick-paste-enabled")
                    .outline()
                    .small()
                    .label(tr(language, "启用快速粘贴快捷键", "Enable quick paste shortcuts"))
                    .selected(quick_paste_enabled)
                    .disabled(quick_paste_pending_setting || paste_shortcuts_pending || !monitoring)
                    .on_click(move |_, _, cx| {
                        let _ = quick_paste_owner.update(cx, |owner, cx| {
                            owner.select_quick_paste_enabled(!owner.quick_paste_enabled, cx);
                        });
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().child(tr(
                        language,
                        "自动粘贴使用按键",
                        "Key sent for automatic paste",
                    )))
                    .child(div().flex().gap_2().children([
                        ("paste-key-ctrl-v", "Ctrl+V", PasteKeyPreference::CtrlV),
                        (
                            "paste-key-shift-insert",
                            "Shift+Insert",
                            PasteKeyPreference::ShiftInsert,
                        ),
                    ]
                    .map(|(id, label, key)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .outline()
                            .small()
                            .child(shortcut_kbd(label).expect("built-in paste key"))
                            .selected(paste_key == key)
                            .disabled(paste_key_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    if owner.paste_key != key
                                        && owner.send(Command::SetPasteKey(key), cx)
                                    {
                                        owner.paste_key_pending = true;
                                        cx.notify();
                                    }
                                });
                            })
                    }))),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(
                        language,
                        "可逐项修改、停用或恢复默认快捷键；数字键自动支持数字小键盘",
                        "Edit, disable, or restore each shortcut; digit keys also work on the numpad",
                    )),
            )
            .when_some(paste_shortcut_status, |panel, (message, is_error)| {
                panel.child(
                    div()
                        .text_xs()
                        .text_color(if is_error {
                            cx.theme().danger
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(message),
                )
            })
            .child(
                Button::new("paste-recent-expand")
                    .outline()
                    .small()
                    .label(tr(language, "普通记录槽位（10）", "Recent slots (10)"))
                    .selected(self.recent_shortcuts_expanded)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.recent_shortcuts_expanded = !this.recent_shortcuts_expanded;
                        cx.notify();
                    })),
            )
            .when(self.recent_shortcuts_expanded, |panel| {
                panel.child(self.paste_group_actions(
                    false,
                    &paste_shortcuts,
                    shortcut_actions_pending,
                    language,
                ))
            })
            .when_some(recent_shortcut_rows, |panel, rows| panel.child(rows))
            .child(
                Button::new("paste-favorite-expand")
                    .outline()
                    .small()
                    .label(tr(language, "收藏槽位（10）", "Favorite slots (10)"))
                    .selected(self.favorite_shortcuts_expanded)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.favorite_shortcuts_expanded = !this.favorite_shortcuts_expanded;
                        cx.notify();
                    })),
            )
            .when(self.favorite_shortcuts_expanded, |panel| {
                panel.child(self.paste_group_actions(
                    true,
                    &paste_shortcuts,
                    shortcut_actions_pending,
                    language,
                ))
            })
            .when_some(favorite_shortcut_rows, |panel, rows| panel.child(rows))
            .into_any_element()
    }
}
