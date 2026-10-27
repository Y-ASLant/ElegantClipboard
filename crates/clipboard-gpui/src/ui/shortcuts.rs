use super::*;

/// 启动时热键装配的全部产物：两条事件通道的收发端与注册结果。
pub(super) struct PasteHotkeyChannels {
    pub(super) paste_sender: async_channel::Sender<PasteHotkeyEvent>,
    pub(super) paste_receiver: async_channel::Receiver<PasteHotkeyEvent>,
    pub(super) sequence_sender: async_channel::Sender<SequenceAdvanceEvent>,
    pub(super) sequence_receiver: async_channel::Receiver<SequenceAdvanceEvent>,
    pub(super) result: Result<Option<(PasteHotkeys, Vec<PasteRegistrationWarning>)>, anyhow::Error>,
}

impl ClipboardView {
    /// 启动时的快速粘贴 + 序列推进键热键装配（决策 0004）：
    /// 创建事件通道并按需注册，返回 (槽位收发, 推进键收发, 注册结果)。
    pub(super) fn register_paste_hotkeys(
        monitoring: bool,
        slots_enabled: bool,
        shortcuts: &PasteShortcutConfig,
        sequence_hotkey: &str,
        main_hotkey: &HotkeyPreference,
    ) -> PasteHotkeyChannels {
        let (paste_sender, paste_receiver) = async_channel::bounded(16);
        let (sequence_sender, sequence_receiver) = async_channel::bounded(4);
        let result = if monitoring {
            PasteHotkeys::start(
                paste_sender.clone(),
                sequence_sender.clone(),
                shortcuts,
                sequence_hotkey,
                slots_enabled,
                main_hotkey,
            )
            .map(Some)
        } else {
            Ok(None)
        };
        PasteHotkeyChannels {
            paste_sender,
            paste_receiver,
            sequence_sender,
            sequence_receiver,
            result,
        }
    }

    /// 统一的快速粘贴 + 序列推进键热键注册入口（决策 0004）。
    /// 返回 (热键实例, 注册警告汇总)。
    pub(super) fn start_paste_hotkeys(
        &self,
    ) -> Result<(PasteHotkeys, Option<String>), anyhow::Error> {
        let (hotkeys, warnings) =
            self.start_paste_hotkeys_with(&self.paste_shortcuts, &self.sequence_hotkey)?;
        Ok((
            hotkeys,
            paste_registration_summary(self.language, &warnings),
        ))
    }

    /// 以指定槽位配置与推进键注册（保存新槽位配置时的预检用）。
    pub(super) fn start_paste_hotkeys_with(
        &self,
        shortcuts: &PasteShortcutConfig,
        sequence_hotkey: &str,
    ) -> Result<(PasteHotkeys, Vec<PasteRegistrationWarning>), anyhow::Error> {
        let (hotkeys, warnings) = PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            self.sequence_hotkey_sender.clone(),
            shortcuts,
            sequence_hotkey,
            self.quick_paste_enabled,
            &self.hotkey_choice,
        )?;
        Ok((hotkeys, warnings))
    }

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

    pub(super) fn select_sequence_hotkey(&mut self, shortcut: String, cx: &mut Context<Self>) {
        if self.sequence_hotkey_pending || shortcut == self.sequence_hotkey {
            return;
        }
        // 预检：与主热键/槽位冲突时拒绝；同时验证格式。
        if let Err(error) = clipboard_platform::hotkey::validate_sequence_shortcut(
            &self.paste_shortcuts,
            &shortcut,
            &self.hotkey_choice,
        ) {
            self.set_feedback_message(error.to_string());
            self.is_error = true;
            cx.notify();
            return;
        }
        let normalized = if shortcut.trim().is_empty() {
            String::new()
        } else {
            match normalize_paste_shortcut(&shortcut) {
                Ok(value) => value,
                Err(error) => {
                    self.set_feedback_message(error.to_string());
                    self.is_error = true;
                    cx.notify();
                    return;
                }
            }
        };
        // 重注册（槽位配置不变，推进键可能变化）；失败回滚。
        let previous = self.sequence_hotkey.clone();
        self.sequence_hotkey = normalized.clone();
        self.paste_hotkeys = None;
        // 未监听时无需注册全局热键，直接保存偏好。
        let saved = if self.monitoring {
            match self.start_paste_hotkeys() {
                Ok((hotkeys, warning)) => {
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning = warning;
                    self.send(Command::SetSequenceHotkey(normalized), cx)
                }
                Err(error) => {
                    self.set_feedback_message(format!(
                        "{}: {error}",
                        tr(
                            self.language,
                            "推进键注册失败",
                            "Failed to register advance key"
                        )
                    ));
                    self.is_error = true;
                    false
                }
            }
        } else {
            self.send(Command::SetSequenceHotkey(normalized), cx)
        };
        if saved {
            self.sequence_hotkey_pending = true;
        } else {
            self.sequence_hotkey = previous;
            if self.monitoring {
                self.paste_hotkeys = None;
                if let Ok((hotkeys, warning)) = self.start_paste_hotkeys() {
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning = warning;
                }
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
            match self.start_paste_hotkeys() {
                Ok((hotkeys, warning)) => {
                    self.paste_hotkeys = Some(hotkeys);
                    self.quick_paste_registration_warning = warning;
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
        } else if let Ok((hotkeys, _)) = self.start_paste_hotkeys() {
            self.paste_hotkeys = Some(hotkeys);
        }
        cx.notify();
    }

    pub(super) fn restore_paste_hotkeys(&mut self) -> Option<String> {
        if !self.monitoring || !self.quick_paste_enabled {
            return None;
        }
        match self.start_paste_hotkeys() {
            Ok((hotkeys, warning)) => {
                self.paste_hotkeys = Some(hotkeys);
                warning
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
            match self.start_paste_hotkeys_with(&next, &self.sequence_hotkey) {
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
