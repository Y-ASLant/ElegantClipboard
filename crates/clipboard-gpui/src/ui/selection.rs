use super::*;

impl ClipboardView {
    pub(super) fn reset_selection(&mut self) {
        self.batch_mode = false;
        self.selected_ids.clear();
        self.selection_anchor = None;
    }
    pub(super) fn exit_batch_mode(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending {
            return;
        }
        self.reset_selection();
        cx.notify();
    }

    /// Flip marks on visited rows, including pages whose metadata was evicted.
    pub(super) fn invert_batch_selection(&mut self, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.batch_mode = true;
        self.selected_ids = self
            .history
            .items
            .known_ids()
            .filter(|id| !self.selected_ids.contains(id))
            .collect();
        self.selection_anchor = None;
        cx.notify();
    }

    pub(super) fn toggle_focused_batch_mark(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.history.selected {
            self.toggle_selection(id, false, window, cx);
        }
    }

    pub(super) fn toggle_selection(
        &mut self,
        id: i64,
        extend_range: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.batch_pending || self.history.loading {
            return;
        }
        if !toggle_batch_mark(
            &mut self.selected_ids,
            &self.history.items,
            self.selection_anchor.filter(|_| extend_range),
            id,
        ) {
            return;
        }
        self.close_hover_preview("batch_selection", cx);
        self.batch_mode = true;
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        self.selection_anchor = Some(id);
        if self.confirmation == Some(HistoryConfirmation::Selection) {
            self.close_history_confirmation(window, cx);
        }
        cx.notify();
    }

    pub(super) fn activate_row(
        &mut self,
        id: i64,
        extend_range: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reorder_pending
            || self.card_drag_active(cx)
            || self.history.loading
            || self.batch_pending
            || self.paste_state.pending()
            || self.editor.id.is_some()
            || !self.history.items.known_ids().any(|known| known == id)
        {
            return;
        }
        self.close_hover_preview("row_activated", cx);
        if self.batch_mode {
            self.toggle_selection(id, extend_range, window, cx);
            return;
        }
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        let can_paste = self.paste_state.target().is_some_and(|target| {
            self._tray.is_some() && paste::is_external_target(window, target)
        });
        if can_paste {
            self.paste_selected(id, window, cx);
        } else {
            self.paste_state.set_target(None);
            self.send(
                Command::Copy {
                    id,
                    mode: CopyMode::Original,
                    purpose: CopyPurpose::Copy,
                },
                cx,
            );
        }
        cx.notify();
    }
    /// 多选快捷键切换：未在多选时进入（勾焦点条，无则空选），已在多选时退出。
    pub(super) fn toggle_batch_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading {
            return;
        }
        if self.batch_mode {
            self.exit_batch_mode(cx);
            return;
        }
        if self.history.items.is_empty() {
            return;
        }
        self.close_hover_preview("batch_enter", cx);
        self.batch_mode = true;
        // Seed with the focused row when nothing is marked yet.
        if self.selected_ids.is_empty()
            && let Some(id) = self.history.selected
            && self.history.items.known_ids().any(|known| known == id)
        {
            self.selected_ids.insert(id);
            self.selection_anchor = Some(id);
        }
        if self.confirmation == Some(HistoryConfirmation::Selection) {
            self.close_history_confirmation(window, cx);
        }
        cx.notify();
    }

    pub(super) fn merge_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.selected_ids.len() < 2 || self.paste_state.pending() {
            return;
        }
        let ids: Vec<_> = self
            .history
            .items
            .known_ids()
            .filter(|id| self.selected_ids.contains(id))
            .collect();
        if ids.len() < 2 {
            return;
        }
        let target = self
            .paste_state
            .target()
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target));
        if self.paste_state.target().is_some() && target.is_none() {
            self.paste_state.set_target(None);
        }
        let purpose = if let Some(target) = target {
            CopyPurpose::Paste {
                request_id: self
                    .paste_state
                    .begin_copy(target, self.paste_wait_from_history()),
            }
        } else {
            CopyPurpose::Copy
        };
        if self.send(Command::Merge { ids, purpose }, cx) {
            self.batch_pending = true;
            self.paste_state.mark_merge_started();
            if self.confirmation == Some(HistoryConfirmation::Selection) {
                self.close_history_confirmation(window, cx);
            }
            self.is_error = false;
            cx.notify();
        } else {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if !self.confirmation_active(HistoryConfirmation::Selection)
            || self.batch_pending
            || self.selected_ids.is_empty()
        {
            return;
        }
        let mut ids: Vec<_> = self.selected_ids.iter().copied().collect();
        ids.sort_unstable();
        if self.send(
            Command::DeleteBatch {
                ids,
                group_id: self.history.group_id(),
                generation: self.history.generation,
            },
            cx,
        ) {
            self.batch_pending = true;
            cx.notify();
        }
    }
}
