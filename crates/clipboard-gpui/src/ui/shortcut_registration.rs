use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShortcutConfig {
    pub(super) main: HotkeyPreference,
    pub(super) slots: PasteShortcutConfig,
    pub(super) sequence: String,
    pub(super) quick: bool,
}

impl ShortcutConfig {
    fn validate(&self) -> anyhow::Result<()> {
        validate_paste_shortcuts(&self.slots, &self.main)?;
        clipboard_platform::hotkey::validate_sequence_shortcut(
            &self.slots,
            &self.sequence,
            &self.main,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ShortcutChangeKind {
    Main,
    Slots,
    Sequence,
    Quick,
}

pub(super) struct ShortcutChange {
    kind: ShortcutChangeKind,
    previous: ShortcutConfig,
    candidate: ShortcutConfig,
}

impl ShortcutChange {
    fn config_after_save(&self, saved: bool) -> &ShortcutConfig {
        if saved {
            &self.candidate
        } else {
            &self.previous
        }
    }
}

impl ClipboardView {
    pub(super) fn shortcut_change_pending(&self) -> bool {
        self.shortcut_change.is_some()
    }

    pub(super) fn shortcut_config(&self) -> ShortcutConfig {
        ShortcutConfig {
            main: self.hotkey_choice.clone(),
            slots: self.paste_shortcuts.clone(),
            sequence: self.sequence_hotkey.clone(),
            quick: self.quick_paste_enabled,
        }
    }

    fn apply_shortcut_config(&mut self, config: &ShortcutConfig) {
        self.hotkey_choice = config.main.clone();
        self.paste_shortcuts = config.slots.clone();
        self.sequence_hotkey = config.sequence.clone();
        self.quick_paste_enabled = config.quick;
    }

    /// 先释放整组旧注册，再按候选值注册，避免推进键与自身注册冲突。
    fn register_shortcut_change(
        &mut self,
        kind: ShortcutChangeKind,
        next: &ShortcutConfig,
        previous: &ShortcutConfig,
    ) -> anyhow::Result<Option<String>> {
        if kind == ShortcutChangeKind::Main {
            self.hotkey = None;
            if next.main != HotkeyPreference::Disabled {
                self.hotkey = Some(Hotkey::start(&next.main, self.hotkey_sender.clone())?);
            }
            return Ok(None);
        }
        self.paste_hotkeys = None;
        if !self.monitoring {
            return Ok(None);
        }
        let (hotkeys, warnings) = PasteHotkeys::start(
            self.paste_hotkey_sender.clone(),
            self.sequence_hotkey_sender.clone(),
            &next.slots,
            &next.sequence,
            next.quick,
            &next.main,
        )?;
        // 推进键失败不能被可用的槽位掩盖；修改槽位失败也必须拒绝保存。
        let failures: Vec<_> = warnings
            .iter()
            .filter(|warning| {
                warning.slot == 0
                    || (kind == ShortcutChangeKind::Slots
                        && next.slots.slot(warning.favorite, warning.slot)
                            != previous.slots.slot(warning.favorite, warning.slot))
            })
            .map(ToString::to_string)
            .collect();
        if !failures.is_empty() {
            drop(hotkeys);
            anyhow::bail!(failures.join("; "));
        }
        self.paste_hotkeys = Some(hotkeys);
        Ok(paste_registration_summary(self.language, &warnings))
    }

    fn restore_shortcut_registration(&mut self, change: &ShortcutChange) -> Option<String> {
        self.register_shortcut_change(change.kind, &change.previous, &change.previous)
            .unwrap_or_else(|error| Some(error.to_string()))
    }

    pub(super) fn begin_shortcut_change(
        &mut self,
        kind: ShortcutChangeKind,
        next: ShortcutConfig,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_change_pending() {
            return;
        }
        let previous = self.shortcut_config();
        if next == previous
            && (kind != ShortcutChangeKind::Main
                || next.main == HotkeyPreference::Disabled
                || self.hotkey.is_some())
        {
            return;
        }
        if let Err(error) = next.validate() {
            self.set_feedback_message(error.to_string());
            self.is_error = true;
            if kind == ShortcutChangeKind::Slots {
                self.paste_shortcut_status = Some((self.message.clone(), true));
            }
            cx.notify();
            return;
        }
        if kind == ShortcutChangeKind::Sequence {
            self.cancel_sequence(cx);
        }
        let change = ShortcutChange {
            kind,
            previous,
            candidate: next,
        };
        let warning = match self.register_shortcut_change(kind, &change.candidate, &change.previous)
        {
            Ok(warning) => warning,
            Err(error) => {
                let restore_error = self.restore_shortcut_registration(&change);
                self.set_feedback_message(error.to_string());
                if let Some(error) = restore_error {
                    self.message.push_str(&format!("; {error}"));
                }
                self.is_error = true;
                if kind == ShortcutChangeKind::Slots {
                    self.paste_shortcut_status = Some((self.message.clone(), true));
                }
                cx.notify();
                return;
            }
        };
        let command = match kind {
            ShortcutChangeKind::Main => Command::SetHotkey(change.candidate.main.clone()),
            ShortcutChangeKind::Slots => {
                Command::SetPasteShortcuts(Box::new(change.candidate.slots.clone()))
            }
            ShortcutChangeKind::Sequence => {
                Command::SetSequenceHotkey(change.candidate.sequence.clone())
            }
            ShortcutChangeKind::Quick => Command::SetQuickPasteEnabled(change.candidate.quick),
        };
        if self.send(command, cx) {
            self.apply_shortcut_config(&change.candidate);
            self.hotkey_pending = kind == ShortcutChangeKind::Main;
            self.sequence_hotkey_pending = kind == ShortcutChangeKind::Sequence;
            self.quick_paste_pending_setting = kind == ShortcutChangeKind::Quick;
            self.paste_shortcuts_pending =
                (kind == ShortcutChangeKind::Slots).then(|| change.previous.slots.clone());
            self.quick_paste_registration_warning = warning;
            self.shortcut_change = Some(change);
        } else {
            if let Some(error) = self.restore_shortcut_registration(&change) {
                self.message.push_str(&format!("; {error}"));
            }
            if kind == ShortcutChangeKind::Slots {
                self.paste_shortcut_status = Some((self.message.clone(), true));
            }
        }
        cx.notify();
    }

    pub(super) fn finish_shortcut_change(
        &mut self,
        kind: ShortcutChangeKind,
        result: Result<(), String>,
        success: String,
        failure: &str,
    ) -> bool {
        if !self
            .shortcut_change
            .as_ref()
            .is_some_and(|change| change.kind == kind)
        {
            return false;
        }
        let Some(change) = self.shortcut_change.take() else {
            return false;
        };
        self.apply_shortcut_config(change.config_after_save(result.is_ok()));
        self.hotkey_pending = false;
        self.sequence_hotkey_pending = false;
        self.quick_paste_pending_setting = false;
        self.paste_shortcuts_pending = None;
        match result {
            Ok(()) => {
                let warning = self.quick_paste_registration_warning.take();
                self.is_error = warning.is_some();
                self.set_feedback_message(warning.unwrap_or(success));
            }
            Err(error) => {
                self.quick_paste_registration_warning = None;
                let restore_error = self.restore_shortcut_registration(&change);
                self.set_feedback_message(format!("{failure}: {error}"));
                if let Some(error) = restore_error {
                    self.message.push_str(&format!("; {error}"));
                }
                self.is_error = true;
            }
        }
        if kind == ShortcutChangeKind::Slots {
            self.paste_shortcut_status = Some((self.message.clone(), self.is_error));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ShortcutConfig {
        ShortcutConfig {
            main: HotkeyPreference::default(),
            slots: PasteShortcutConfig::default(),
            sequence: "Ctrl+Shift+G".into(),
            quick: false,
        }
    }

    #[::core::prelude::v1::test]
    fn quick_enable_uses_candidate_flag_and_disable_preserves_sequence() {
        let previous = config();
        let candidate = ShortcutConfig {
            quick: true,
            ..previous.clone()
        };
        let change = ShortcutChange {
            kind: ShortcutChangeKind::Quick,
            previous: previous.clone(),
            candidate: candidate.clone(),
        };
        assert!(change.config_after_save(true).quick);
        assert_eq!(change.config_after_save(false), &previous);
        let disable = ShortcutChange {
            kind: ShortcutChangeKind::Quick,
            previous: candidate,
            candidate: previous.clone(),
        };
        assert!(!disable.config_after_save(true).quick);
        assert_eq!(disable.config_after_save(true).sequence, "Ctrl+Shift+G");
        assert!(disable.config_after_save(false).quick);
    }

    #[::core::prelude::v1::test]
    fn failed_main_slot_and_sequence_saves_restore_the_previous_configuration() {
        let previous = config();
        for kind in [
            ShortcutChangeKind::Main,
            ShortcutChangeKind::Slots,
            ShortcutChangeKind::Sequence,
        ] {
            let mut candidate = previous.clone();
            match kind {
                ShortcutChangeKind::Main => {
                    candidate.main = HotkeyPreference::Shortcut("Ctrl+Alt+F8".into())
                }
                ShortcutChangeKind::Slots => candidate
                    .slots
                    .set_slot(false, 1, "Ctrl+Alt+F8".into())
                    .unwrap(),
                ShortcutChangeKind::Sequence => candidate.sequence = "Ctrl+Alt+F8".into(),
                ShortcutChangeKind::Quick => unreachable!(),
            }
            let change = ShortcutChange {
                kind,
                previous: previous.clone(),
                candidate: candidate.clone(),
            };
            assert_eq!(change.config_after_save(false), &previous);
            assert_eq!(change.config_after_save(true), &candidate);
        }
    }

    #[::core::prelude::v1::test]
    fn every_direction_rejects_three_way_conflicts_even_when_quick_paste_is_disabled() {
        let mut next = config();
        next.main = HotkeyPreference::Shortcut(next.sequence.clone());
        assert!(next.validate().is_err());
        next = config();
        next.slots
            .set_slot(false, 1, next.sequence.clone())
            .unwrap();
        assert!(next.validate().is_err());
        next = config();
        next.sequence = next.slots.slot(false, 1).unwrap().into();
        assert!(next.validate().is_err());
        next = config();
        next.sequence = match next.main.clone() {
            HotkeyPreference::Shortcut(value) => value,
            HotkeyPreference::Disabled => unreachable!(),
        };
        assert!(next.validate().is_err());
        next.sequence.clear();
        assert!(next.validate().is_ok());
    }
}
