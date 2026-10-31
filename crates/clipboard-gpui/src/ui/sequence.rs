use super::*;

impl ClipboardView {
    pub(super) fn start_sequence_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcut_change_pending()
            || self.batch_pending
            || self.paste_state.pending()
            || self.paste_state.quick_pending()
            || self.paste_state.sequence_remaining().is_some()
        {
            return;
        }
        if self.sequence_hotkey.trim().is_empty() {
            self.set_feedback_message(
                tr(
                    self.language,
                    "序列粘贴推进键已停用，请在设置 → 快捷键中启用",
                    "The sequence-paste advance key is disabled; enable it in Settings → Shortcuts",
                )
                .into(),
            );
            self.is_error = true;
            cx.notify();
            return;
        }
        let Some(target) = self
            .paste_state
            .target()
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target))
        else {
            self.paste_state.set_target(None);
            self.set_feedback_message(tr(self.language,
                "请从目标应用按全局快捷键唤出，再使用序列粘贴",
                "Open the clipboard with the global shortcut from the target app before sequence pasting").into());
            self.is_error = true;
            cx.notify();
            return;
        };
        let ids: Vec<_> = self
            .history
            .items
            .iter()
            .filter(|item| self.selected_ids.contains(&item.id))
            .map(|item| item.id)
            .collect();
        if ids.len() < 2 {
            return;
        }
        self.close_hover_preview("sequence_started", cx);
        if !self.paste_state.start_sequence(ids, target) {
            return;
        }
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        if !self.advance_sequence(window, cx) {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn advance_sequence(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.paste_state.pending() || self.paste_state.quick_pending() || self.batch_pending {
            return false;
        }
        let Some(target) = self.paste_state.sequence_target() else {
            return false;
        };
        if !paste::is_external_target(window, target) {
            return false;
        }
        let Some(command) = self.paste_state.advance_sequence() else {
            return false;
        };
        // 发送前记录请求归属，首条复制不会把自己的序列当作用户打断。
        if !self.send(command, cx) {
            self.interrupt_paste_except(None);
            return false;
        }
        if let Some(remaining) = self.paste_state.sequence_remaining() {
            let message = if self.language == LanguagePreference::English {
                format!(
                    "Sequence paste: {remaining} left (advance with {})",
                    self.sequence_hotkey
                )
            } else {
                format!(
                    "序列粘贴中：剩余 {remaining} 条（推进键 {}）",
                    self.sequence_hotkey
                )
            };
            self.set_feedback_message(message);
            self.is_error = false;
        }
        cx.notify();
        true
    }

    pub(super) fn sequence_paste_delivered(&mut self, request_id: u64, cx: &mut Context<Self>) {
        if let Some(total) = self.paste_state.complete_delivery(request_id) {
            self.reset_selection();
            self.set_feedback_message(if self.language == LanguagePreference::English {
                format!("Sequence paste finished: {total} items")
            } else {
                format!("序列粘贴完成，共 {total} 条")
            });
            self.is_error = false;
        }
        cx.notify();
    }

    pub(super) fn cancel_sequence(&mut self, cx: &mut Context<Self>) {
        if self.paste_state.sequence_remaining().is_some() {
            self.interrupt_paste_except(None);
            self.set_feedback_message(
                tr(self.language, "序列粘贴已取消", "Sequence paste cancelled").into(),
            );
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn handle_sequence_advance(
        &mut self,
        target: (isize, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_change_pending()
            || self.paste_state.sequence_remaining().is_none()
            || self.paste_state.pending()
        {
            return;
        }
        if !self.paste_state.sequence_keeps_target(Some(target))
            || !paste::is_external_target(window, target)
        {
            self.cancel_sequence(cx);
            return;
        }
        if !self.advance_sequence(window, cx) {
            self.cancel_sequence(cx);
        }
    }
}
