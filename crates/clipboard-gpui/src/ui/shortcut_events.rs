use super::shortcut_registration::ShortcutChangeKind;
use super::*;

impl ClipboardView {
    pub(super) fn apply_hotkey_saved(&mut self, result: Result<HotkeyPreference, String>) -> bool {
        let success = if self.hotkey_choice == HotkeyPreference::Disabled {
            tr(
                self.language,
                "全局快捷键已关闭，可从托盘唤出窗口",
                "The global shortcut is disabled; open the window from the tray.",
            )
            .into()
        } else {
            String::new()
        };
        self.finish_shortcut_change(
            ShortcutChangeKind::Main,
            result.map(|_| ()),
            success,
            tr(self.language, "快捷键保存失败", "Failed to save shortcut"),
        )
    }

    pub(super) fn apply_sequence_hotkey_saved(&mut self, result: Result<String, String>) -> bool {
        self.finish_shortcut_change(
            ShortcutChangeKind::Sequence,
            result.map(|_| ()),
            String::new(),
            tr(
                self.language,
                "保存序列粘贴推进键失败",
                "Failed to save sequence paste advance key",
            ),
        )
    }

    pub(super) fn apply_quick_paste_enabled_saved(&mut self, result: Result<bool, String>) -> bool {
        self.finish_shortcut_change(
            ShortcutChangeKind::Quick,
            result.map(|_| ()),
            String::new(),
            tr(
                self.language,
                "保存快速粘贴快捷键设置失败",
                "Failed to save quick paste shortcuts setting",
            ),
        )
    }

    pub(super) fn apply_paste_shortcuts_saved(
        &mut self,
        result: Result<Box<PasteShortcutConfig>, String>,
    ) -> bool {
        self.finish_shortcut_change(
            ShortcutChangeKind::Slots,
            result.map(|_| ()),
            String::new(),
            tr(
                self.language,
                "保存快速粘贴槽位快捷键失败",
                "Failed to save quick paste slot shortcut",
            ),
        )
    }

    pub(super) fn apply_quick_paste_resolved(
        &mut self,
        request_id: u64,
        result: Result<i64, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(quick) = self.paste_state.resolve_quick(request_id) else {
            return false;
        };
        match result {
            Ok(id) if paste::is_external_target(window, quick.target) => {
                let wait = self.paste_wait_for_slot(quick.slot, quick.favorite);
                let request_id = self.paste_state.begin_copy(quick.target, wait);
                if !self.send(
                    Command::Copy {
                        id,
                        mode: CopyMode::Original,
                        purpose: CopyPurpose::Paste { request_id },
                    },
                    cx,
                ) {
                    self.interrupt_paste_except(None);
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
