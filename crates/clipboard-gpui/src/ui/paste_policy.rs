use super::*;
use clipboard_core::preferences::PasteDeliveryMode;
use clipboard_platform::paste_delivery::PasteWaitPolicy;

impl ClipboardView {
    pub(super) fn paste_wait_from_history(&self) -> PasteWaitPolicy {
        PasteWaitPolicy::for_hotkey(self.paste_delivery_mode.value, None)
    }

    fn paste_wait_for_hotkey(&self, trigger: Option<String>) -> PasteWaitPolicy {
        PasteWaitPolicy::for_hotkey(self.paste_delivery_mode.value, trigger)
    }

    pub(super) fn paste_wait_for_slot(&self, slot: u8, favorite: bool) -> PasteWaitPolicy {
        let trigger = self
            .paste_shortcuts
            .slot(favorite, slot)
            .map(str::to_string);
        self.paste_wait_for_hotkey(trigger)
    }

    pub(super) fn paste_wait_for_sequence(&self) -> PasteWaitPolicy {
        let trigger = (!self.sequence_hotkey.trim().is_empty())
            .then(|| self.sequence_hotkey.clone());
        self.paste_wait_for_hotkey(trigger)
    }

    pub(super) fn paste_slots_allow_repeat(&self) -> bool {
        self.paste_delivery_mode.value == PasteDeliveryMode::Rapid
    }
}
