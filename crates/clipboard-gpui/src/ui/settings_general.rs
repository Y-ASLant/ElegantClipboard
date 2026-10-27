use super::settings_common::{command_checkbox, owner_state};
use super::*;
use gpui_kit::component::{alert::Alert, switch::Switch};

impl SettingsWindowView {
    pub(super) fn settings_shortcut_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let pending = state.hotkey_pending
            || state.paste_shortcuts_pending.is_some()
            || state.quick_paste_pending_setting
            || crate::capabilities::desktop_unavailable_reason().is_some();
        let disabled = state.hotkey_choice == HotkeyPreference::Disabled;
        let label = hotkey_label(language, &state.hotkey_choice).to_owned();
        let owner = self.owner.clone();
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
                    .child(if disabled {
                        div().child(label).into_any_element()
                    } else {
                        shortcut_kbd(&label).map_or_else(
                            || div().child(label).into_any_element(),
                            |kbd| kbd.into_any_element(),
                        )
                    })
                    .child(
                        Button::new("main-shortcut-edit")
                            .small()
                            .outline()
                            .label(tr(language, "修改", "Edit"))
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_shortcut_edit(ShortcutTarget::Main, window, cx);
                            })),
                    )
                    .child(
                        Button::new("main-shortcut-disable")
                            .small()
                            .ghost()
                            .label(tr(language, "关闭", "Disable"))
                            .disabled(pending || disabled)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    owner.select_hotkey(HotkeyPreference::Disabled, cx);
                                });
                            }),
                    ),
            )
            .when_some(
                crate::capabilities::desktop_unavailable_reason(),
                |panel, reason| {
                    panel.child(Alert::warning("global-shortcut-capability", reason).small())
                },
            )
            .when(
                self.shortcut_editing == Some(ShortcutTarget::Main),
                |panel| panel.child(self.shortcut_editor(language, pending, cx)),
            )
            .into_any_element()
    }

    pub(super) fn settings_behavior_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        // (id, 中文标签, 英文标签, 命令, (选中值, pending) 读取器, 发送成功后的回调)
        type BehaviorToggle = (
            &'static str,
            &'static str,
            &'static str,
            fn(bool) -> Command,
            fn(&ClipboardView) -> (bool, bool),
            fn(&mut ClipboardView),
        );
        let behavior_toggles: [BehaviorToggle; 7] = [
            (
                "persist-window-size",
                "记住窗口大小",
                "Remember window size",
                Command::SetPersistWindowSize,
                |state| (state.persist_window_size, state.persist_window_size_pending),
                |owner| {
                    owner.persist_window_size_pending = true;
                    owner.window_size_task = None;
                },
            ),
            (
                "auto-reset-state",
                "隐藏时重置搜索、筛选和滚动",
                "Reset search, filters and scroll on hide",
                Command::SetAutoResetState,
                |state| (state.auto_reset_state.value, state.auto_reset_state.pending),
                |owner| owner.auto_reset_state.pending = true,
            ),
            (
                "search-auto-focus",
                "唤出时聚焦搜索",
                "Focus search when shown",
                Command::SetSearchAutoFocus,
                |state| {
                    (
                        state.search_auto_focus.value,
                        state.search_auto_focus.pending,
                    )
                },
                |owner| owner.search_auto_focus.pending = true,
            ),
            (
                "search-auto-clear",
                "唤出时清空搜索",
                "Clear search when shown",
                Command::SetSearchAutoClear,
                |state| {
                    (
                        state.search_auto_clear.value,
                        state.search_auto_clear.pending,
                    )
                },
                |owner| owner.search_auto_clear.pending = true,
            ),
            (
                "skip-clear-confirm",
                "清理历史免确认",
                "Clear history without confirmation",
                Command::SetSkipClearConfirm,
                |state| {
                    (
                        state.skip_clear_confirm.value,
                        state.skip_clear_confirm.pending,
                    )
                },
                |owner| owner.skip_clear_confirm.pending = true,
            ),
            (
                "paste-close-window",
                "粘贴后关闭窗口",
                "Close after paste",
                Command::SetPasteCloseWindow,
                |state| {
                    (
                        state.paste_close_window.value,
                        state.paste_close_window.pending,
                    )
                },
                |owner| owner.paste_close_window.pending = true,
            ),
            (
                "paste-move-to-top",
                "粘贴后移到列表首位",
                "Move to top after paste",
                Command::SetPasteMoveToTop,
                |state| {
                    (
                        state.paste_move_to_top.value,
                        state.paste_move_to_top.pending,
                    )
                },
                |owner| owner.paste_move_to_top.pending = true,
            ),
        ];
        div()
            .flex()
            .w_full()
            .flex_wrap()
            .items_start()
            .gap_3()
            .children(
                behavior_toggles.map(|(id, chinese, english, command, state_of, on_sent)| {
                    let (checked, pending) = state_of(owner_state);
                    command_checkbox(
                        id,
                        tr(language, chinese, english),
                        checked,
                        pending,
                        &self.owner,
                        command,
                        on_sent,
                    )
                }),
            )
            .into_any_element()
    }

    pub(super) fn sequence_shortcut_editor(
        &self,
        language: LanguagePreference,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.shortcut_editor(language, self.sequence_hotkey_pending(cx), cx)
    }

    fn sequence_hotkey_pending(&self, cx: &Context<Self>) -> bool {
        self.owner
            .upgrade()
            .is_some_and(|owner| owner.read(cx).sequence_hotkey_pending)
    }

    pub(super) fn settings_startup_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let autostart = state.autostart.value;
        let autostart_pending = state.autostart.pending;
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
                                owner.autostart.pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
            .into_any_element()
    }
}
