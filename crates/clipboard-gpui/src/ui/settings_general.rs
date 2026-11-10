use super::settings_common::{
    command_checkbox, command_checkbox_help, owner_state, settings_column, settings_columns,
    settings_inline, settings_shortcut_row, settings_stack, settings_toggle,
};
use super::*;
use gpui_kit::component::{alert::Alert, switch::Switch};

impl SettingsWindowView {
    pub(super) fn settings_shortcut_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let pending = state.shortcut_change_pending()
            || crate::capabilities::global_shortcut_unavailable_reason(language).is_some();
        let disabled = state.hotkey_choice == HotkeyPreference::Disabled;
        let label = hotkey_label(language, &state.hotkey_choice).to_owned();
        let shortcut = if disabled {
            String::new()
        } else {
            label.clone()
        };
        let owner = self.owner.clone();
        settings_stack()
            .child(
                settings_shortcut_row()
                    .child(shortcut_binding_display(shortcut.as_str(), label))
                    .child(
                        Button::new("main-shortcut-edit")
                            .outline()
                            .small()
                            .label(tr(language, "修改", "Edit"))
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.begin_shortcut_edit(ShortcutTarget::Main, window, cx);
                            })),
                    )
                    .child(
                        Button::new("main-shortcut-disable")
                            .ghost()
                            .small()
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
                crate::capabilities::global_shortcut_unavailable_reason(language),
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
        // (id, 中文标签, 英文标签, 命令, (选中值, pending) 读取器, 发送成功后的回调, 可选悬停说明)
        type BehaviorToggle = (
            &'static str,
            &'static str,
            &'static str,
            fn(bool) -> Command,
            fn(&ClipboardView) -> (bool, bool),
            fn(&mut ClipboardView),
            Option<(&'static str, &'static str)>,
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
                None,
            ),
            (
                "auto-reset-state",
                "隐藏时重置搜索、筛选和滚动",
                "Reset search, filters and scroll on hide",
                Command::SetAutoResetState,
                |state| (state.auto_reset_state.value, state.auto_reset_state.pending),
                |owner| owner.auto_reset_state.pending = true,
                Some((
                    "窗口隐藏时一并清空搜索词、分类筛选并滚回顶部；覆盖「唤出时清空搜索」的唤出场景。",
                    "Hiding the window also clears the search, category filter and scroll position; it already covers the \"Clear search when shown\" case.",
                )),
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
                Some((
                    "开启后唤出窗口时焦点落在搜索框，直接输入即可搜索；关闭则聚焦列表便于键盘选择。",
                    "When enabled, showing the window focuses the search box so typing filters immediately; when off, focus stays on the list for keyboard selection.",
                )),
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
                Some((
                    "只清空搜索词、不动筛选与滚动；窗口未隐藏、只是唤回前台时也生效，适合长时间保持窗口显示的场景。",
                    "Clears only the search text, leaving the filter and scroll position; also applies when the visible window is merely brought back to front, which suits users who keep the window open.",
                )),
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
                None,
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
                None,
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
                None,
            ),
        ];
        let make_checkbox =
            |(id, chinese, english, command, state_of, on_sent, help): BehaviorToggle| {
                let (checked, pending) = state_of(owner_state);
                command_checkbox_help(
                    cx,
                    id,
                    tr(language, chinese, english),
                    checked,
                    pending,
                    &self.owner,
                    command,
                    on_sent,
                    help.map(|(chinese, english)| tr(language, chinese, english)),
                )
                .w_full()
                .min_w_0()
            };
        // 复选框行高 32px，列基宽 240：内容区 ≥512px（2×240+间距）即可两列并排。
        let split_at = behavior_toggles.len().div_ceil(2);
        let behavior_column = || {
            settings_column()
                .flex_basis(px(240.))
                .gap(px(visual::SPACE_2))
        };
        settings_columns()
            .child(
                behavior_column().children(
                    behavior_toggles[..split_at]
                        .iter()
                        .copied()
                        .map(make_checkbox),
                ),
            )
            .child(
                behavior_column().children(
                    behavior_toggles[split_at..]
                        .iter()
                        .copied()
                        .map(make_checkbox),
                ),
            )
            .into_any_element()
    }

    pub(super) fn settings_diagnostics_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        settings_stack()
            .child(
                command_checkbox(
                    cx,
                    "diagnostics-enabled",
                    tr(language, "记录诊断日志", "Enable diagnostic logs"),
                    state.diagnostics_enabled.value,
                    state.diagnostics_enabled.pending,
                    &self.owner,
                    Command::SetDiagnosticsEnabled,
                    |owner| owner.diagnostics_enabled.pending = true,
                )
                .w_full()
                .min_w_0(),
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
            .is_some_and(|owner| owner.read(cx).shortcut_change_pending())
    }

    pub(super) fn settings_startup_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let autostart = state.autostart.value;
        let autostart_pending = state.autostart.pending;
        let startup_owner = self.owner.clone();
        let panel = settings_inline().child(
            settings_toggle(cx, Switch::new("settings-window-autostart"))
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
        );
        #[cfg(windows)]
        let panel = {
            let run_as_admin = state.run_as_admin.value;
            let run_as_admin_pending = state.run_as_admin.pending;
            let is_admin = state.is_admin;
            let admin_owner = self.owner.clone();
            panel.child(
                settings_toggle(cx, Switch::new("settings-window-run-as-admin"))
                    .label(tr(
                        language,
                        "以管理员身份启动",
                        "Run as administrator",
                    ))
                    .checked(run_as_admin)
                    .disabled(run_as_admin_pending)
                    .on_change(move |checked, window, cx| {
                        if *checked && !is_admin {
                            let owner = admin_owner.clone();
                            window.open_dialog(cx, move |dialog, _, cx| {
                                let elevate_owner = owner.clone();
                                let later_owner = owner.clone();
                                dialog_shell(dialog, cx)
                                    .title(dialog_title(
                                        tr(
                                            language,
                                            "以管理员身份启动",
                                            "Run as administrator",
                                        ),
                                        cx,
                                    ))
                                    .child(dialog_copy().child(tr(
                                        language,
                                        "需要管理员权限才能监听并粘贴到以管理员身份运行的程序（如部分游戏、终端）。保存后立即重启为管理员，还是下次启动时生效？",
                                        "Administrator rights are required to monitor and paste into elevated apps (some games, terminals). Restart as administrator now, or apply on next launch?",
                                    )))
                                    .footer(
                                        dialog_footer(cx)
                                            .child(
                                                Button::new("admin-cancel")
                                                    .outline()
                                                    .label(tr(language, "取消", "Cancel"))
                                                    .on_click(|_, window, cx| {
                                                        window.close_dialog(cx)
                                                    }),
                                            )
                                            .child(
                                                Button::new("admin-later")
                                                    .outline()
                                                    .label(tr(
                                                        language,
                                                        "下次启动",
                                                        "Next launch",
                                                    ))
                                                    .on_click(move |_, window, cx| {
                                                        let _ = later_owner.update(cx, |owner, cx| {
                                                            if owner.send(
                                                                Command::SetRunAsAdmin(true),
                                                                cx,
                                                            ) {
                                                                owner.run_as_admin.pending = true;
                                                            }
                                                        });
                                                        window.close_dialog(cx);
                                                    }),
                                            )
                                            .child(
                                                Button::new("admin-now")
                                                    .primary()
                                                    .label(tr(
                                                        language,
                                                        "立即重启",
                                                        "Restart now",
                                                    ))
                                                    .on_click(move |_, window, cx| {
                                                        let _ = elevate_owner.update(
                                                            cx,
                                                            |owner, cx| {
                                                                owner
                                                                    .relaunch_as_admin(cx);
                                                            },
                                                        );
                                                        window.close_dialog(cx);
                                                    }),
                                            ),
                                    )
                            });
                            return;
                        }
                        let _ = admin_owner.update(cx, |owner, cx| {
                            if owner.send(Command::SetRunAsAdmin(*checked), cx) {
                                owner.run_as_admin.pending = true;
                                cx.notify();
                            }
                        });
                    }),
            )
        };
        panel.into_any_element()
    }
}
