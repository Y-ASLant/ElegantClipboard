use super::*;
use gpui_kit::component::{accordion::Accordion, alert::Alert, checkbox::Checkbox, switch::Switch};

impl SettingsWindowView {
    pub(super) fn settings_shortcut_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (language, hotkey_choice, hotkey_pending) = {
            let state = owner_entity.read(cx);
            (state.language, state.hotkey_choice, state.hotkey_pending)
        };

        let choices = [
            HotkeyPreference::CtrlShiftV,
            HotkeyPreference::AltC,
            HotkeyPreference::CtrlAltV,
            HotkeyPreference::Disabled,
        ];
        let selected_index = choices
            .iter()
            .position(|choice| *choice == hotkey_choice)
            .expect("built-in hotkey choice");
        let click_choices = choices;
        let owner = self.owner.clone();
        TabBar::new("settings-window-hotkey-tabs")
            .w_full()
            .outline()
            .selected_index(selected_index)
            .on_click(move |index: &usize, _, cx| {
                if let Some(&choice) = click_choices.get(*index) {
                    let _ = owner.update(cx, |owner, cx| {
                        if owner.hotkey_choice != choice {
                            owner.select_hotkey(choice, cx);
                        }
                    });
                }
            })
            .children(choices.map(|choice| {
                let tab = Tab::new().flex_1().disabled(hotkey_pending);
                if choice == HotkeyPreference::Disabled {
                    tab.label(hotkey_label(language, choice))
                } else {
                    tab.aria_label(choice.label()).child(
                        shortcut_kbd(choice.label())
                            .expect("built-in hotkey")
                            .appearance(false),
                    )
                }
            }))
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

        let choices = [
            (
                tr(language, "跟随光标", "Follow cursor"),
                WindowPositionPreference::FollowCursor,
            ),
            (
                tr(language, "当前屏幕居中", "Center on screen"),
                WindowPositionPreference::ScreenCenter,
            ),
            (
                tr(language, "保持位置", "Keep position"),
                WindowPositionPreference::FixedPosition,
            ),
        ];
        let selected_index = choices
            .iter()
            .position(|(_, preference)| *preference == window_position)
            .expect("built-in window position");
        let click_choices = choices;
        let owner = self.owner.clone();
        TabBar::new("settings-window-position-tabs")
            .w_full()
            .segmented()
            .selected_index(selected_index)
            .on_click(move |index: &usize, _, cx| {
                if let Some(&(_, preference)) = click_choices.get(*index) {
                    let _ = owner.update(cx, |owner, cx| {
                        if owner.window_position != preference
                            && owner.send(Command::SetWindowPosition(preference), cx)
                        {
                            owner.window_position_pending = true;
                            cx.notify();
                        }
                    });
                }
            })
            .children(choices.map(|(label, _)| {
                Tab::new()
                    .label(label)
                    .flex_1()
                    .disabled(window_position_pending)
            }))
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
            .w_full()
            .flex_wrap()
            .items_start()
            .gap_3()
            .child(
                Checkbox::new("persist-window-size")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(language, "记住窗口大小", "Remember window size"))
                    .checked(persist_window_size)
                    .disabled(persist_window_size_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = persist_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetPersistWindowSize(*checked), cx) {
                                owner.persist_window_size_pending = true;
                                owner.window_size_task = None;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("auto-reset-state")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(
                        language,
                        "隐藏时重置搜索、筛选和滚动",
                        "Reset search, filters and scroll on hide",
                    ))
                    .checked(auto_reset_state)
                    .disabled(auto_reset_state_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = reset_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetAutoResetState(*checked), cx) {
                                owner.auto_reset_state_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("search-auto-focus")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(language, "唤出时聚焦搜索", "Focus search when shown"))
                    .checked(search_auto_focus)
                    .disabled(search_auto_focus_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = search_focus_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetSearchAutoFocus(*checked), cx) {
                                owner.search_auto_focus_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("search-auto-clear")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(language, "唤出时清空搜索", "Clear search when shown"))
                    .checked(search_auto_clear)
                    .disabled(search_auto_clear_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = search_clear_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetSearchAutoClear(*checked), cx) {
                                owner.search_auto_clear_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("skip-clear-confirm")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(
                        language,
                        "清理历史免确认",
                        "Clear history without confirmation",
                    ))
                    .checked(skip_clear_confirm)
                    .disabled(skip_clear_confirm_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = skip_clear_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetSkipClearConfirm(*checked), cx) {
                                owner.skip_clear_confirm_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("paste-close-window")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(language, "粘贴后关闭窗口", "Close after paste"))
                    .checked(paste_close_window)
                    .disabled(paste_close_window_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = paste_close_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetPasteCloseWindow(*checked), cx) {
                                owner.paste_close_window_pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                Checkbox::new("paste-move-to-top")
                    .text_sm()
                    .w(px(350.))
                    .label(tr(
                        language,
                        "粘贴后移到列表首位",
                        "Move to top after paste",
                    ))
                    .checked(paste_move_to_top)
                    .disabled(paste_move_to_top_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = paste_move_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetPasteMoveToTop(*checked), cx) {
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
                Switch::new("settings-window-autostart")
                    .label(tr(language, "开机自启", "Start on login"))
                    .checked(autostart)
                    .disabled(autostart_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = startup_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetAutostart(*checked), cx) {
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
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(
                Checkbox::new("quick-paste-enabled")
                    .text_sm()
                    .label(tr(language, "启用快速粘贴快捷键", "Enable quick paste shortcuts"))
                    .checked(quick_paste_enabled)
                    .disabled(quick_paste_pending_setting || paste_shortcuts_pending || !monitoring)
                    .on_change(move |checked, _, cx| {
                        let _ = quick_paste_owner.update(cx, |owner, cx| {
                            owner.select_quick_paste_enabled(*checked, cx);
                        });
                    }),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().child(tr(
                        language,
                        "自动粘贴使用按键",
                        "Key sent for automatic paste",
                    )))
                    .child({
                        let choices = [
                            ("Ctrl+V", PasteKeyPreference::CtrlV),
                            ("Shift+Insert", PasteKeyPreference::ShiftInsert),
                        ];
                        let selected_index = choices
                            .iter()
                            .position(|(_, key)| *key == paste_key)
                            .expect("built-in paste key");
                        let click_choices = choices;
                        let owner = self.owner.clone();
                        TabBar::new("settings-paste-key-tabs")
                            .w_full()
                            .segmented()
                            .selected_index(selected_index)
                            .on_click(move |index: &usize, _, cx| {
                                if let Some(&(_, key)) = click_choices.get(*index) {
                                    let _ = owner.update(cx, |owner, cx| {
                                        if owner.paste_key != key
                                            && owner.send(Command::SetPasteKey(key), cx)
                                        {
                                            owner.paste_key_pending = true;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .children(choices.map(|(label, _)| {
                                Tab::new()
                                    .aria_label(label)
                                    .child(
                                        shortcut_kbd(label)
                                            .expect("built-in paste key")
                                            .appearance(false),
                                    )
                                    .flex_1()
                                    .disabled(paste_key_pending)
                            }))
                    }),
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
                panel.child(if is_error {
                    Alert::error("paste-shortcut-status", message).small()
                } else {
                    Alert::info("paste-shortcut-status", message).small()
                })
            })
            .child(
                Accordion::new("paste-shortcut-groups")
                    .w_full()
                    .multiple(true)
                    .small()
                    .on_toggle_click({
                        let settings = cx.entity().downgrade();
                        move |indices, _, cx| {
                            let _ = settings.update(cx, |this, cx| {
                                let recent = indices.contains(&0);
                                let favorite = indices.contains(&1);
                                if this.recent_shortcuts_expanded != recent
                                    || this.favorite_shortcuts_expanded != favorite
                                {
                                    this.recent_shortcuts_expanded = recent;
                                    this.favorite_shortcuts_expanded = favorite;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .item(|item| {
                        item.title(tr(language, "普通记录槽位（10）", "Recent slots (10)"))
                            .open(self.recent_shortcuts_expanded)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when(self.recent_shortcuts_expanded, |content| {
                                        content
                                            .child(self.paste_group_actions(
                                                false,
                                                &paste_shortcuts,
                                                shortcut_actions_pending,
                                                language,
                                            ))
                                            .when_some(recent_shortcut_rows, |content, rows| {
                                                content.child(rows)
                                            })
                                    }),
                            )
                    })
                    .item(|item| {
                        item.title(tr(language, "收藏槽位（10）", "Favorite slots (10)"))
                            .open(self.favorite_shortcuts_expanded)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when(self.favorite_shortcuts_expanded, |content| {
                                        content
                                            .child(self.paste_group_actions(
                                                true,
                                                &paste_shortcuts,
                                                shortcut_actions_pending,
                                                language,
                                            ))
                                            .when_some(favorite_shortcut_rows, |content, rows| {
                                                content.child(rows)
                                            })
                                    }),
                            )
                    }),
            )
            .into_any_element()
    }
}
