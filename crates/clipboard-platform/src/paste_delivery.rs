use clipboard_core::preferences::PasteDeliveryMode;
use std::time::Duration;

pub const PASTE_INJECT_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteWaitPolicy {
    pub mode: PasteDeliveryMode,
    pub trigger_shortcut: Option<String>,
}

impl PasteWaitPolicy {
    pub fn controlled() -> Self {
        Self {
            mode: PasteDeliveryMode::Controlled,
            trigger_shortcut: None,
        }
    }

    pub fn for_hotkey(mode: PasteDeliveryMode, trigger: Option<String>) -> Self {
        Self {
            mode,
            trigger_shortcut: if mode == PasteDeliveryMode::Rapid {
                trigger
            } else {
                None
            },
        }
    }

    pub fn ready_to_inject(
        &self,
        modifiers_held: bool,
        trigger_held: bool,
        elapsed: Duration,
    ) -> ReadyResult {
        if elapsed >= PASTE_INJECT_WAIT {
            return ReadyResult::TimedOut;
        }
        let ready = match (self.mode, self.trigger_shortcut.as_deref()) {
            (PasteDeliveryMode::Controlled, _) => !modifiers_held,
            (PasteDeliveryMode::Rapid, Some(_)) => !trigger_held,
            (PasteDeliveryMode::Rapid, None) => !modifiers_held,
        };
        if ready {
            ReadyResult::Ready
        } else {
            ReadyResult::Waiting
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadyResult {
    Waiting,
    Ready,
    TimedOut,
}

pub use crate::hotkey::paste_shortcut_trigger_held;

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::preferences::PasteDeliveryMode;
    use std::time::Duration;

    #[test]
    fn controlled_mode_waits_for_modifiers() {
        let policy = PasteWaitPolicy::controlled();
        assert_eq!(
            policy.ready_to_inject(true, false, Duration::ZERO),
            ReadyResult::Waiting
        );
        assert_eq!(
            policy.ready_to_inject(false, true, Duration::ZERO),
            ReadyResult::Ready
        );
    }

    #[test]
    fn rapid_hotkey_waits_for_trigger_not_modifiers() {
        let policy = PasteWaitPolicy::for_hotkey(
            PasteDeliveryMode::Rapid,
            Some("Alt+1".into()),
        );
        assert_eq!(
            policy.ready_to_inject(true, true, Duration::ZERO),
            ReadyResult::Waiting
        );
        assert_eq!(
            policy.ready_to_inject(true, false, Duration::ZERO),
            ReadyResult::Ready
        );
    }
}
