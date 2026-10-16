use super::*;

impl ClipboardView {
    pub(super) fn apply_hotkey_saved(&mut self, result: Result<HotkeyPreference, String>) -> bool {
        self.hotkey_pending = false;
        match result {
            Ok(choice) => {
                self.hotkey_choice = choice.clone();
                self.set_feedback_message(if choice == HotkeyPreference::Disabled {
                    tr(
                        self.language,
                        "全局快捷键已关闭，可从托盘唤出窗口",
                        "The global shortcut is disabled; open the window from the tray.",
                    )
                    .into()
                } else if self.language == LanguagePreference::English {
                    format!("Shortcut saved: {}", hotkey_label(self.language, &choice))
                } else {
                    format!("已保存唤出快捷键：{}", choice.label())
                });
                self.is_error = false;
            }
            Err(error) => {
                self.hotkey = None;
                let restore_error = self.restore_hotkey();
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "快捷键保存失败", "Failed to save shortcut")
                ));
                if let Some(restore_error) = restore_error {
                    self.message.push_str(&format!("; {restore_error}"));
                }
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn apply_quick_paste_enabled_saved(&mut self, result: Result<bool, String>) -> bool {
        self.quick_paste_pending_setting = false;
        match result {
            Ok(enabled) => {
                self.quick_paste_enabled = enabled;
                if let Some(warning) = self.quick_paste_registration_warning.take() {
                    self.set_feedback_message(warning);
                    self.is_error = true;
                } else {
                    self.set_feedback_message(
                        tr(
                            self.language,
                            "快速粘贴快捷键设置已保存",
                            "Quick paste shortcuts setting saved",
                        )
                        .into(),
                    );
                    self.is_error = false;
                }
            }
            Err(error) => {
                self.quick_paste_registration_warning = None;
                if self.quick_paste_enabled && self.paste_hotkeys.is_none() {
                    if let Ok((hotkeys, _)) = PasteHotkeys::start(
                        self.paste_hotkey_sender.clone(),
                        &self.paste_shortcuts,
                        &self.hotkey_choice,
                    ) {
                        self.paste_hotkeys = Some(hotkeys);
                    }
                } else if !self.quick_paste_enabled {
                    self.paste_hotkeys = None;
                }
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存快速粘贴快捷键设置失败",
                        "Failed to save quick paste shortcuts setting",
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_paste_shortcuts_saved(
        &mut self,
        result: Result<Box<PasteShortcutConfig>, String>,
    ) -> bool {
        let Some(previous) = self.paste_shortcuts_pending.take() else {
            return false;
        };
        match result {
            Ok(shortcuts) => {
                self.paste_shortcuts = *shortcuts;
                if let Some(warning) = self.quick_paste_registration_warning.take() {
                    self.set_feedback_message(warning);
                    self.is_error = true;
                } else {
                    self.set_feedback_message(
                        tr(
                            self.language,
                            "快速粘贴槽位快捷键已保存",
                            "Quick paste slot shortcut saved",
                        )
                        .into(),
                    );
                    self.is_error = false;
                }
                self.paste_shortcut_status = Some((self.message.clone(), self.is_error));
            }
            Err(error) => {
                self.paste_shortcuts = previous;
                self.paste_hotkeys = None;
                self.quick_paste_registration_warning = None;
                let restore_error = self.restore_paste_hotkeys();
                self.set_feedback_message(error);
                if let Some(error) = restore_error {
                    self.message.push_str(&format!("; {error}"));
                }
                self.is_error = true;
                self.paste_shortcut_status = Some((self.message.clone(), true));
            }
        }
        true
    }

    pub(super) fn apply_quick_paste_resolved(
        &mut self,
        slot: u8,
        favorite: bool,
        result: Result<i64, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((pending_slot, pending_favorite, target)) = self.quick_paste_pending.take() else {
            return false;
        };
        if slot != pending_slot || favorite != pending_favorite {
            self.quick_paste_pending = Some((pending_slot, pending_favorite, target));
            return false;
        }
        match result {
            Ok(id) if paste::is_external_target(window, target) => {
                if self.send(Command::CopyForPaste(id), cx) {
                    self.paste_pending = Some((id, target));
                }
            }
            Ok(_) => {
                self.set_feedback_message(
                    tr(
                        self.language,
                        "目标窗口已变化，快速粘贴已取消",
                        "Target window changed; quick paste was canceled",
                    )
                    .into(),
                );
                self.is_error = true;
            }
            Err(error) => {
                self.set_feedback_message(error);
                self.is_error = true;
            }
        }
        true
    }
}
