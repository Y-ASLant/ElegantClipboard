use super::*;

/// 历史窗口隐藏后，序列仍保留开始时的目标窗口。
pub(super) struct SequenceQueue {
    ids: Vec<i64>,
    target: (isize, u32),
    started: usize,
    delivered: usize,
    request: Option<(u64, i64)>,
}

impl SequenceQueue {
    pub(super) fn new(ids: Vec<i64>, target: (isize, u32)) -> Self {
        Self {
            ids,
            target,
            started: 0,
            delivered: 0,
            request: None,
        }
    }

    pub(super) fn remaining(&self) -> usize {
        self.ids.len() - self.delivered
    }

    fn next_id(&self) -> Option<i64> {
        self.request
            .is_none()
            .then(|| self.ids.get(self.started).copied())
            .flatten()
    }

    fn begin(&mut self, request_id: u64) -> Option<i64> {
        let id = self.next_id()?;
        self.request = Some((request_id, id));
        self.started += 1;
        Some(id)
    }

    pub(super) fn owns_command(&self, command: &Command) -> bool {
        matches!(command, Command::CopyForPaste(id, request_id)
            if self.request == Some((*request_id, *id)))
    }

    fn delivered(&mut self, request_id: u64) -> bool {
        if self.request.map(|(request, _)| request) != Some(request_id) {
            return false;
        }
        self.request = None;
        self.delivered += 1;
        self.delivered == self.ids.len()
    }

    fn keeps_target(&self, target: Option<(isize, u32)>) -> bool {
        target == Some(self.target)
    }
}

impl ClipboardView {
    pub(super) fn start_sequence_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcut_change_pending()
            || self.batch_pending
            || self.paste_pending.is_some()
            || self.quick_paste_pending.is_some()
            || self.sequence_queue.is_some()
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
            .paste_target
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target))
        else {
            self.paste_target = None;
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
        self.close_hover_preview(cx);
        self.sequence_queue = Some(SequenceQueue::new(ids, target));
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        if !self.advance_sequence(window, cx) {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn advance_sequence(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.paste_pending.is_some() || self.quick_paste_pending.is_some() || self.batch_pending
        {
            return false;
        }
        let Some(queue) = self.sequence_queue.as_ref() else {
            return false;
        };
        if queue.next_id().is_none() || !paste::is_external_target(window, queue.target) {
            return false;
        }
        let request_id = self.paste_pending.begin(queue.target);
        let Some(id) = self
            .sequence_queue
            .as_mut()
            .and_then(|queue| queue.begin(request_id))
        else {
            self.interrupt_paste_except(None);
            return false;
        };
        // 发送前记录请求归属，首条复制不会把自己的序列当作用户打断。
        if !self.send(Command::CopyForPaste(id, request_id), cx) {
            self.interrupt_paste_except(None);
            return false;
        }
        if let Some(queue) = &self.sequence_queue {
            let remaining = queue.remaining();
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
        let Some(queue) = &mut self.sequence_queue else {
            return;
        };
        if queue.delivered(request_id) {
            let total = queue.ids.len();
            self.sequence_queue = None;
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
        if self.sequence_queue.is_some() {
            self.interrupt_paste_except(None);
            self.set_feedback_message(
                tr(self.language, "序列粘贴已取消", "Sequence paste cancelled").into(),
            );
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn sequence_keeps_target(&self, target: Option<(isize, u32)>) -> bool {
        self.sequence_queue
            .as_ref()
            .is_some_and(|queue| queue.keeps_target(target))
    }

    pub(super) fn handle_sequence_advance(
        &mut self,
        target: (isize, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shortcut_change_pending()
            || self.sequence_queue.is_none()
            || self.paste_pending.is_some()
        {
            return;
        }
        if !self.sequence_keeps_target(Some(target)) || !paste::is_external_target(window, target) {
            self.cancel_sequence(cx);
            return;
        }
        if !self.advance_sequence(window, cx) {
            self.cancel_sequence(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn first_copy_keeps_its_queue_and_advance_waits_until_delivery() {
        let mut queue = SequenceQueue::new(vec![10, 20], (1, 2));
        assert_eq!(queue.begin(7), Some(10));
        assert!(queue.owns_command(&Command::CopyForPaste(10, 7)));
        assert_eq!(queue.next_id(), None);
        assert_eq!(queue.begin(8), None);
        assert_eq!(queue.remaining(), 2);
        assert!(!queue.delivered(6));
        assert_eq!(queue.next_id(), None);
        assert!(!queue.delivered(7));
        assert_eq!(queue.next_id(), Some(20));
        assert_eq!(queue.remaining(), 1);
        assert_eq!(queue.begin(8), Some(20));
        assert!(!queue.delivered(7));
        assert!(queue.delivered(8));
        assert_eq!(queue.next_id(), None);
        assert_eq!(queue.remaining(), 0);
        assert!(!queue.delivered(8));
    }

    #[::core::prelude::v1::test]
    fn return_to_original_target_preserves_sequence_but_other_targets_do_not() {
        let mut queue = SequenceQueue::new(vec![10, 20], (1, 2));
        queue.begin(3);
        assert!(queue.keeps_target(Some((1, 2))));
        queue.delivered(3);
        assert!(queue.keeps_target(Some((1, 2))));
        for target in [None, Some((1, 3)), Some((2, 2))] {
            assert!(!queue.keeps_target(target));
        }
        assert_eq!(queue.next_id(), Some(20));
        assert!(!queue.owns_command(&Command::CopyForPaste(10, 4)));
        assert!(!queue.owns_command(&Command::CopyAsFormat {
            id: 10,
            format: CopyFormat::Html
        }));
        assert!(!queue.owns_command(&Command::CopyTransformed {
            id: 10,
            transform: TextTransform::Uppercase
        }));
    }
}
