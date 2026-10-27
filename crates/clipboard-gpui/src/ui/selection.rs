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
    pub(super) fn toggle_selection(
        &mut self,
        id: i64,
        extend_range: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_pending_row_click();
        if self.batch_pending || self.history.loading {
            return;
        }
        if !self.history.items.iter().any(|item| item.id == id) {
            return;
        }
        self.close_hover_preview(cx);
        self.batch_mode = true;
        let ids: Vec<_> = self.history.items.iter().map(|item| item.id).collect();
        let range = extend_range
            .then_some(self.selection_anchor)
            .flatten()
            .and_then(|anchor| selection_range_ids(&ids, anchor, id));
        if let Some(range) = range {
            self.selected_ids.extend(range.iter().copied());
        } else if !self.selected_ids.insert(id) {
            self.selected_ids.remove(&id);
        }
        self.selection_anchor = Some(id);
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        cx.notify();
    }

    pub(super) fn cancel_pending_row_click(&mut self) {
        self.pending_row_click = None;
        self.row_click_task = None;
    }

    pub(super) fn handle_row_click(
        &mut self,
        id: i64,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reorder_pending
            || self.card_drag_active(cx)
            || self.history.loading
            || self.batch_pending
            || self.paste_pending.is_some()
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        if !event.is_keyboard() && self.drag_release_pending {
            return;
        }
        let editable = self.history.items.iter().any(|item| {
            item.id == id && matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf")
        });
        if self.batch_mode || !editable || event.is_keyboard() {
            self.cancel_pending_row_click();
            self.activate_row(id, event.modifiers().shift, window, cx);
            return;
        }
        if event.click_count() >= 2 {
            self.cancel_pending_row_click();
            if !self.history.loading && !self.batch_pending && self.paste_pending.is_none() {
                self.open_preview_for_edit(id, window, cx);
            }
            return;
        }
        self.cancel_pending_row_click();
        let generation = self.history.generation;
        self.close_hover_preview(cx);
        self.pending_row_click = Some((id, generation));
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        let delay = input::double_click_interval().max(Duration::from_millis(1))
            + Duration::from_millis(30);
        let main_window = window.window_handle();
        self.row_click_task = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(delay).await;
            let _ = update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                if this.pending_row_click == Some((id, generation))
                    && this.history.generation == generation
                    && this.preview.id.is_none()
                    && !this.batch_mode
                {
                    this.cancel_pending_row_click();
                    this.activate_row(id, false, window, cx);
                }
            });
        }));
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
            || self.paste_pending.is_some()
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.history.selected = Some(id);
        window.focus(&self.list_focus, cx);
        if self.batch_mode {
            self.toggle_selection(id, extend_range, window, cx);
            return;
        }
        let can_paste = self.paste_target.is_some_and(|target| {
            self._tray.is_some() && paste::is_external_target(window, target)
        });
        if can_paste {
            self.paste_selected(id, window, cx);
        } else {
            self.paste_target = None;
            self.send(Command::Copy(id), cx);
        }
        cx.notify();
    }
    pub(super) fn select_all_loaded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.batch_mode = true;
        self.selected_ids
            .extend(self.history.items.iter().map(|item| item.id));
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        cx.notify();
    }

    pub(super) fn merge_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.batch_pending || self.selected_ids.len() < 2 || self.paste_pending.is_some() {
            return;
        }
        let ids: Vec<_> = self
            .history
            .items
            .iter()
            .filter_map(|item| self.selected_ids.contains(&item.id).then_some(item.id))
            .collect();
        if ids.len() < 2 {
            return;
        }
        let target = self
            .paste_target
            .filter(|target| self._tray.is_some() && paste::is_external_target(window, *target));
        if self.paste_target.is_some() && target.is_none() {
            self.paste_target = None;
        }
        let command = if let Some(target) = target {
            let request_id = self.paste_pending.begin(target);
            Command::MergeForPaste { ids, request_id }
        } else {
            Command::MergeCopy(ids)
        };
        if self.send(command, cx) {
            self.batch_pending = true;
            self.batch_paste_pending = target;
            if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
                self.close_history_confirmation(window, cx);
            }
            self.set_feedback_message(
                if target.is_some() {
                    tr(
                        self.language,
                        "正在合并并返回原窗口…",
                        "Merging and returning to the previous window…",
                    )
                } else {
                    tr(
                        self.language,
                        "正在合并所选记录…",
                        "Merging selected items…",
                    )
                }
                .into(),
            );
            self.is_error = false;
            cx.notify();
        } else {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if !self.confirmation_active(HistoryConfirmation::DeleteSelected)
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
