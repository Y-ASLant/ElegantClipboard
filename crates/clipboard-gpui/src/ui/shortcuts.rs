use super::shortcut_registration::ShortcutChangeKind;
use super::*;

pub(super) struct PasteHotkeyChannels {
    pub(super) paste_sender: async_channel::Sender<PasteHotkeyEvent>,
    pub(super) paste_receiver: async_channel::Receiver<PasteHotkeyEvent>,
    pub(super) sequence_sender: async_channel::Sender<SequenceAdvanceEvent>,
    pub(super) sequence_receiver: async_channel::Receiver<SequenceAdvanceEvent>,
    pub(super) result: Result<Option<(PasteHotkeys, Vec<PasteRegistrationWarning>)>, anyhow::Error>,
}

impl ClipboardView {
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

    pub(super) fn select_hotkey(&mut self, choice: HotkeyPreference, cx: &mut Context<Self>) {
        if choice != self.hotkey_choice || self.hotkey.is_none() {
            self.onboarding_hotkey_used = false;
        }
        let mut next = self.shortcut_config();
        next.main = choice;
        self.begin_shortcut_change(ShortcutChangeKind::Main, next, cx);
    }

    pub(super) fn select_sequence_hotkey(&mut self, shortcut: String, cx: &mut Context<Self>) {
        match normalize_paste_shortcut(&shortcut) {
            Ok(shortcut) => {
                let mut next = self.shortcut_config();
                next.sequence = shortcut;
                self.begin_shortcut_change(ShortcutChangeKind::Sequence, next, cx);
            }
            Err(error) => {
                self.set_feedback_message(error.to_string());
                self.is_error = true;
                cx.notify();
            }
        }
    }

    pub(super) fn select_quick_paste_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if !self.monitoring {
            return;
        }
        let mut next = self.shortcut_config();
        next.quick = enabled;
        self.begin_shortcut_change(ShortcutChangeKind::Quick, next, cx);
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
        slots: PasteShortcutConfig,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_change_pending() {
            return;
        }
        self.paste_shortcut_status = None;
        let mut next = self.shortcut_config();
        next.slots = slots;
        self.begin_shortcut_change(ShortcutChangeKind::Slots, next, cx);
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
            || self.onboarding_visible()
            || !self.quick_paste_enabled
            || self.shortcut_change_pending()
            || self.paste_hotkeys.is_none()
            || self.paste_state.quick_pending()
            || self.paste_state.pending()
            || self.batch_pending
            || self.paste_state.merging()
            || !paste::is_external_target(window, event.target)
        {
            return;
        }
        let request_id = self.paste_state.begin_quick(event.target);
        if !self.send(
            Command::ResolveQuickPaste {
                request_id,
                slot: event.slot,
                favorite: event.favorite,
                group_id: self.history.group_id(),
            },
            cx,
        ) {
            self.paste_state.cancel_quick();
        }
    }
}
