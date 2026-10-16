use super::*;

impl ClipboardView {
    pub(super) fn restore_hotkey(&mut self) -> Option<String> {
        if self.hotkey_choice == HotkeyPreference::Disabled {
            return None;
        }
        match Hotkey::start(&self.hotkey_choice, self.hotkey_sender.clone()) {
            Ok(hotkey) => {
                self.hotkey = Some(hotkey);
                None
            }
            Err(error) => Some(format!(
                "{}: {error}",
                tr(
                    self.language,
                    "原快捷键也无法恢复",
                    "The previous shortcut could not be restored"
                )
            )),
        }
    }

    pub(super) fn select_hotkey(&mut self, choice: HotkeyPreference, cx: &mut Context<Self>) {
        if self.hotkey_pending
            || self.quick_paste_pending_setting
            || self.paste_shortcuts_pending.is_some()
            || (choice == self.hotkey_choice
                && (choice == HotkeyPreference::Disabled || self.hotkey.is_some()))
        {
            return;
        }
        if let Err(error) = validate_paste_shortcuts(&self.paste_shortcuts, &choice) {
            self.set_feedback_message(error.to_string());
            self.is_error = true;
            cx.notify();
            return;
        }
        self.hotkey = None;
        let registration = if choice == HotkeyPreference::Disabled {
            Ok(None)
        } else {
            Hotkey::start(&choice, self.hotkey_sender.clone()).map(Some)
        };
        match registration {
            Ok(hotkey) => self.hotkey = hotkey,
            Err(error) => {
                let restore_error = self.restore_hotkey();
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "快捷键切换失败", "Failed to change shortcut")
                ));
                if let Some(restore_error) = restore_error {
                    self.message.push_str(&format!("；{restore_error}"));
                }
                self.is_error = true;
                cx.notify();
                return;
            }
        }
        if self.send(Command::SetHotkey(choice), cx) {
            self.hotkey_pending = true;
        } else {
            self.hotkey = None;
            if let Some(error) = self.restore_hotkey() {
                self.message.push_str(&format!("；{error}"));
            }
        }
        cx.notify();
    }

    pub(super) fn select_quick_paste_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if !self.monitoring
            || self.hotkey_pending
            || self.quick_paste_pending_setting
            || self.paste_shortcuts_pending.is_some()
            || enabled == self.quick_paste_enabled
        {
            return;
        }
        if enabled {
            match PasteHotkeys::start(
                self.paste_hotkey_sender.clone(),
                &self.paste_shortcuts,
                &self.hotkey_choice,
            ) {
                Ok((hotkeys, warnings)) => {
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning =
                        paste_registration_summary(self.language, &warnings);
                }
                Err(error) => {
                    self.set_feedback_message(error.to_string());
                    self.is_error = true;
                    cx.notify();
                    return;
                }
            }
        } else {
            self.paste_hotkeys = None;
            self.quick_paste_registration_warning = None;
        }
        if self.send(Command::SetQuickPasteEnabled(enabled), cx) {
            self.quick_paste_pending_setting = true;
        } else if enabled {
            self.paste_hotkeys = None;
            self.quick_paste_registration_warning = None;
        } else if let Ok((hotkeys, _)) = PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            &self.paste_shortcuts,
            &self.hotkey_choice,
        ) {
            self.paste_hotkeys = Some(hotkeys);
        }
        cx.notify();
    }

    pub(super) fn restore_paste_hotkeys(&mut self) -> Option<String> {
        if !self.monitoring || !self.quick_paste_enabled {
            return None;
        }
        match PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            &self.paste_shortcuts,
            &self.hotkey_choice,
        ) {
            Ok((hotkeys, warnings)) => {
                self.paste_hotkeys = Some(hotkeys);
                paste_registration_summary(self.language, &warnings)
            }
            Err(error) => Some(error.to_string()),
        }
    }

    pub(super) fn save_paste_shortcut(
        &mut self,
        favorite: bool,
        slot: u8,
        shortcut: &str,
        cx: &mut Context<Self>,
    ) {
        let next = (|| {
            let normalized = normalize_paste_shortcut(shortcut)?;
            let mut next = self.paste_shortcuts.clone();
            next.set_slot(favorite, slot, normalized)?;
            Ok::<_, anyhow::Error>(next)
        })();
        match next {
            Ok(next) => self.save_paste_shortcuts(next, cx),
            Err(error) => {
                self.set_feedback_message(error.to_string());
                self.is_error = true;
                self.paste_shortcut_status = Some((self.message.clone(), true));
                cx.notify();
            }
        }
    }

    pub(super) fn save_paste_shortcuts(
        &mut self,
        next: PasteShortcutConfig,
        cx: &mut Context<Self>,
    ) {
        if self.paste_shortcuts_pending.is_some()
            || self.quick_paste_pending_setting
            || self.hotkey_pending
        {
            return;
        }
        self.paste_shortcut_status = None;
        if let Err(error) = validate_paste_shortcuts(&next, &self.hotkey_choice) {
            self.set_feedback_message(error.to_string());
            self.is_error = true;
            self.paste_shortcut_status = Some((self.message.clone(), true));
            cx.notify();
            return;
        }
        if next == self.paste_shortcuts {
            return;
        }
        self.paste_hotkeys = None;
        if self.monitoring && self.quick_paste_enabled {
            match PasteHotkeys::start(self.paste_hotkey_sender.clone(), &next, &self.hotkey_choice)
            {
                Ok((hotkeys, warnings)) => {
                    let changed_slot_failed = warnings.iter().any(|warning| {
                        next.slot(warning.favorite, warning.slot)
                            != self.paste_shortcuts.slot(warning.favorite, warning.slot)
                    });
                    if changed_slot_failed {
                        drop(hotkeys);
                        let restore_error = self.restore_paste_hotkeys();
                        self.set_feedback_message(
                            warnings
                                .iter()
                                .filter(|warning| {
                                    next.slot(warning.favorite, warning.slot)
                                        != self.paste_shortcuts.slot(warning.favorite, warning.slot)
                                })
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("; "),
                        );
                        if let Some(error) = restore_error {
                            self.message.push_str(&format!("; {error}"));
                        }
                        self.is_error = true;
                        self.paste_shortcut_status = Some((self.message.clone(), true));
                        cx.notify();
                        return;
                    }
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning =
                        paste_registration_summary(self.language, &warnings);
                }
                Err(error) => {
                    let restore_error = self.restore_paste_hotkeys();
                    self.set_feedback_message(error.to_string());
                    if let Some(error) = restore_error {
                        self.message.push_str(&format!("; {error}"));
                    }
                    self.is_error = true;
                    self.paste_shortcut_status = Some((self.message.clone(), true));
                    cx.notify();
                    return;
                }
            }
        }
        let previous = self.paste_shortcuts.clone();
        if self.send(Command::SetPasteShortcuts(Box::new(next.clone())), cx) {
            self.paste_shortcuts = next;
            self.paste_shortcuts_pending = Some(previous);
        } else {
            self.paste_hotkeys = None;
            if let Some(error) = self.restore_paste_hotkeys() {
                self.message.push_str(&format!("; {error}"));
            }
            self.paste_shortcut_status = Some((self.message.clone(), true));
            self.quick_paste_registration_warning = None;
        }
        cx.notify();
    }

    pub(super) fn set_paste_shortcut_group_defaults(
        &mut self,
        favorite: bool,
        reset: bool,
        cx: &mut Context<Self>,
    ) {
        let mut next = self.paste_shortcuts.clone();
        let defaults = PasteShortcutConfig::default();
        let group = if favorite {
            &mut next.favorites
        } else {
            &mut next.recent
        };
        for (index, shortcut) in group.iter_mut().enumerate() {
            *shortcut = if reset {
                if favorite {
                    defaults.favorites[index].clone()
                } else {
                    defaults.recent[index].clone()
                }
            } else {
                String::new()
            };
        }
        self.save_paste_shortcuts(next, cx);
    }

    pub(super) fn handle_quick_hotkey(
        &mut self,
        event: PasteHotkeyEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.monitoring
            || !self.quick_paste_enabled
            || self.paste_hotkeys.is_none()
            || self.quick_paste_pending.is_some()
            || self.paste_pending.is_some()
            || self.batch_pending
            || self.batch_paste_pending.is_some()
            || !paste::is_external_target(window, event.target)
        {
            return;
        }
        let request_id = self.quick_paste_pending.begin(event.target);
        if !self.send(
            Command::ResolveQuickPaste {
                request_id,
                slot: event.slot,
                favorite: event.favorite,
                group_id: self.history.group_id(),
            },
            cx,
        ) {
            self.quick_paste_pending.take();
        }
    }
}
