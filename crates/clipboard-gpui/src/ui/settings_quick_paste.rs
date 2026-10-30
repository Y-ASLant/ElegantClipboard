use super::settings_common::{
    owner_state, settings_column, settings_columns, settings_description, settings_field,
    settings_inline, settings_stack,
};
use super::*;
use gpui_kit::component::{accordion::Accordion, alert::Alert, checkbox::Checkbox};

impl SettingsWindowView {
    pub(super) fn settings_quick_paste_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = state.language;
        let quick_paste_enabled = state.quick_paste_enabled;
        let paste_key = state.paste_key.value;
        let paste_key_pending = state.paste_key.pending;
        let paste_shortcuts = state.paste_shortcuts.clone();
        let shortcut_actions_pending = state.shortcut_change_pending()
            || crate::capabilities::paste_unavailable_reason(language).is_some();
        let paste_shortcut_status = state.paste_shortcut_status.clone();
        let monitoring = state.monitoring;
        let quick_paste_owner = self.owner.clone();
        let sequence_hotkey = state.sequence_hotkey.clone();
        let sequence_pending = shortcut_actions_pending;
        let sequence_owner = self.owner.clone();
        let sequence_label = if sequence_hotkey.trim().is_empty() {
            tr(language, "已停用", "Disabled").to_owned()
        } else {
            sequence_hotkey.clone()
        };
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
        settings_stack()
            .child(
                Checkbox::new("quick-paste-enabled")
                    .min_h(px(visual::CONTROL_HEIGHT_LARGE))
                    .py(px(6.))
                    .text_size(px(14.))
                    .label(tr(language, "启用快速粘贴快捷键", "Enable quick paste shortcuts"))
                    .checked(quick_paste_enabled)
                    .disabled(shortcut_actions_pending || !monitoring)
                    .on_change(move |checked, _, cx| {
                        let _ = quick_paste_owner.update(cx, |owner, cx| {
                            owner.select_quick_paste_enabled(*checked, cx);
                        });
                    }),
            )
            .when_some(crate::capabilities::paste_unavailable_reason(language), |panel, reason| {
                panel.child(Alert::warning("paste-capability", reason).small())
            })
            // 序列粘贴推进键（决策 0004）：录制或停用，与主热键/槽位互斥。
            .child(settings_columns().child(
                settings_column().child(settings_field(tr(
                    language,
                    "序列粘贴推进键（多选后在目标应用中逐条粘贴）",
                    "Sequence paste advance key (paste selected items one by one)",
                ))
                    .child(
                        settings_inline()
                            .child(if sequence_hotkey.trim().is_empty() {
                                div().child(sequence_label).into_any_element()
                            } else {
                                shortcut_kbd(&sequence_label).map_or_else(
                                    || div().child(sequence_label).into_any_element(),
                                    |kbd| kbd.into_any_element(),
                                )
                            })
                            .child(
                                Button::new("sequence-shortcut-edit")
                                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                                    .outline()
                                    .label(tr(language, "修改", "Edit"))
                                    .disabled(sequence_pending)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.begin_shortcut_edit(
                                            ShortcutTarget::Sequence,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new("sequence-shortcut-disable")
                                    .h(px(visual::CONTROL_HEIGHT_LARGE))
                                    .ghost()
                                    .label(tr(language, "停用", "Disable"))
                                    .disabled(sequence_pending || sequence_hotkey.trim().is_empty())
                                    .on_click(move |_, _, cx| {
                                        let _ = sequence_owner.update(cx, |owner, cx| {
                                            owner.select_sequence_hotkey(String::new(), cx);
                                        });
                                    }),
                            ),
                    )
                    .when(
                        self.shortcut_editing == Some(ShortcutTarget::Sequence),
                        |panel| {
                            panel.child(self.sequence_shortcut_editor(language, cx))
                        },
                    )),
            ).child(
                settings_column()
            .when(cfg!(target_os = "macos"), |panel| {
                panel.child(Alert::info("mac-paste-key", tr(language,
                    "macOS 自动粘贴使用 Command+V，不支持 Shift+Insert。请在系统设置中授予辅助功能权限。",
                    "macOS uses Command+V; Shift+Insert is unavailable. Grant Accessibility permission in System Settings.")).small())
            })
            .child(
                settings_field(tr(
                    language,
                    "自动粘贴使用按键",
                    "Key sent for automatic paste",
                ))
                    .child({
                        let choices = [
                            (crate::capabilities::paste_key_label(), PasteKeyPreference::CtrlV),
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
                            .min_w_0()
                            .large()
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .segmented()
                            .last_empty_space(div().w_0())
                            .selected_index(selected_index)
                            .on_click(move |index: &usize, _, cx| {
                                if let Some(&(_, key)) = click_choices.get(*index) {
                                    if paste_key_pending || !crate::capabilities::paste_key_supported(key) {
                                        return;
                                    }
                                    let _ = owner.update(cx, |owner, cx| {
                                        if owner.paste_key.value != key
                                            && owner.send(Command::SetPasteKey(key), cx)
                                        {
                                            owner.paste_key.pending = true;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .children(choices.map(|(label, key)| {
                                Tab::new()
                                    .text_size(px(visual::TEXT_BODY))
                                    .aria_label(label)
                                    .child(
                                        shortcut_kbd(label)
                                            .expect("built-in paste key")
                                            .appearance(false),
                                    )
                                    .flex_1()
                                    .disabled(paste_key_pending || !crate::capabilities::paste_key_supported(key))
                            }))
                    }),
            )))
            .child(
                settings_description(tr(
                    language,
                    "可逐项修改、停用或恢复默认快捷键；数字键自动支持数字小键盘",
                    "Edit, disable, or restore each shortcut; digit keys also work on the numpad",
                ), cx),
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
                                settings_stack()
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
                                settings_stack()
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
