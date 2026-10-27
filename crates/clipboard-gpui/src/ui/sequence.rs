use super::*;

/// 序列粘贴队列（决策 0004）：待贴条目按列表顺序逐条受控粘贴。
pub(super) struct SequenceQueue {
    pub(super) ids: Vec<i64>,
    /// 已发起受控粘贴的条目数（含进行中的一条）。
    pub(super) started: usize,
    /// 已成功送达目标的条目数。
    pub(super) delivered: usize,
}

impl SequenceQueue {
    pub(super) fn new(ids: Vec<i64>) -> Self {
        Self {
            ids,
            started: 0,
            delivered: 0,
        }
    }

    pub(super) fn remaining(&self) -> usize {
        self.ids.len() - self.started
    }

    pub(super) fn next_id(&self) -> Option<i64> {
        self.ids.get(self.started).copied()
    }
}

impl ClipboardView {
    /// 进入序列粘贴：要求有效的原窗口目标与推进键；立即送出第一条。
    pub(super) fn start_sequence_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.paste_pending.is_some() || self.sequence_queue.is_some() {
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
        let target = self
            .paste_target
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target));
        if target.is_none() {
            self.paste_target = None;
            self.set_feedback_message(tr(
                self.language,
                "请从目标应用按全局快捷键唤出，再使用序列粘贴",
                "Open the clipboard with the global shortcut from the target app before sequence pasting",
            ).into());
            self.is_error = true;
            cx.notify();
            return;
        }
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
        self.sequence_queue = Some(sequence::SequenceQueue::new(ids));
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        // 送出第一条；失败即取消序列。
        if !self.advance_sequence(window, cx) {
            self.cancel_sequence(cx);
        }
    }

    /// 送出队列中的下一条。返回 false 表示队列已尽或发送失败。
    /// 返回 true 时窗口已隐藏、粘贴流程在后台推进。
    pub(super) fn advance_sequence(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(queue) = &mut self.sequence_queue else {
            return false;
        };
        let Some(id) = queue.next_id() else {
            return false;
        };
        let remaining = queue.remaining();
        // 复用 paste_selected 的完整受控粘贴校验；它隐藏窗口并发送粘贴键。
        self.paste_selected(id, window, cx);
        if self.paste_pending.is_none() {
            return false;
        }
        let queue = self.sequence_queue.as_mut().expect("queue checked above");
        queue.started += 1;
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
        true
    }

    /// 粘贴送达确认后推进队列；队列耗尽时结束序列。
    pub(super) fn sequence_paste_delivered(&mut self, cx: &mut Context<Self>) {
        let Some(queue) = &mut self.sequence_queue else {
            return;
        };
        queue.delivered += 1;
        if queue.delivered >= queue.ids.len() {
            let total = queue.ids.len();
            self.sequence_queue = None;
            self.reset_selection();
            let message = if self.language == LanguagePreference::English {
                format!("Sequence paste finished: {total} items")
            } else {
                format!("序列粘贴完成，共 {total} 条")
            };
            self.set_feedback_message(message);
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn cancel_sequence(&mut self, cx: &mut Context<Self>) {
        if self.sequence_queue.take().is_some() {
            self.reset_selection();
            self.set_feedback_message(
                tr(self.language, "序列粘贴已取消", "Sequence paste cancelled").into(),
            );
            self.is_error = false;
            cx.notify();
        }
    }

    /// 推进键热键事件：仅在序列激活且目标仍有效时推进。
    pub(super) fn handle_sequence_advance(
        &mut self,
        target: (isize, u32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sequence_queue.is_none() || self.paste_pending.is_some() {
            return;
        }
        if self.paste_target != Some(target) || !paste::is_external_target(window, target) {
            self.cancel_sequence(cx);
            return;
        }
        if !self.advance_sequence(window, cx) {
            self.cancel_sequence(cx);
        }
    }
}
