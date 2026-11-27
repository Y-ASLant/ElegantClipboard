use super::*;

impl HistoryConfirmation {
    fn matches_scope(self, group_id: Option<i64>, has_selection: bool) -> bool {
        match self {
            Self::Group(id) => group_id == Some(id),
            Self::Selection => has_selection,
            // Item presence is checked in confirmation_active.
            Self::Item(_) => true,
        }
    }
}

impl ClipboardView {
    /// Confirm before removing one history row (action bar, menu, Delete key).
    pub(super) fn request_delete_item(
        &mut self,
        id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.loading
            || self.batch_pending
            || self.batch_mode
            || !self.history.items.known_ids().any(|known| known == id)
        {
            return;
        }
        self.history.selected = Some(id);
        self.open_history_confirmation(HistoryConfirmation::Item(id), window, cx);
    }
}

impl ClipboardView {
    pub(super) fn clear_all_history(&mut self, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        let main = self.main_window;
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = owner.update(cx, |view, cx| view.confirm_clear_all_history(window, cx));
            });
        });
    }

    fn confirm_clear_all_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clear_all_pending
            || !self.close_history_editor_for_action(self.editor.id.is_some(), window, cx)
        {
            return;
        }
        self.clear_all_error = None;
        if self.send(Command::ClearAllHistory, cx) {
            self.clear_all_pending = true;
        } else {
            self.clear_all_error = Some(self.message.clone());
        }
        cx.notify();
    }
    pub(super) fn confirmation_active(&self, kind: HistoryConfirmation) -> bool {
        if self.confirmation != Some(kind) {
            return false;
        }
        match kind {
            HistoryConfirmation::Item(id) => {
                self.history.items.known_ids().any(|known| known == id)
            }
            other => other.matches_scope(self.history.group_id(), !self.selected_ids.is_empty()),
        }
    }

    pub(super) fn confirmation_pending(&self, kind: HistoryConfirmation) -> bool {
        match kind {
            HistoryConfirmation::Group(_) => self.group_delete_pending,
            HistoryConfirmation::Selection => self.batch_pending,
            HistoryConfirmation::Item(_) => false,
        }
    }

    pub(super) fn cancel_confirmation(
        &mut self,
        kind: HistoryConfirmation,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.confirmation != Some(kind) || self.confirmation_pending(kind) {
            return false;
        }
        self.confirmation = None;
        self.confirmation_error = None;
        cx.notify();
        true
    }

    fn close_history_editor_for_action(
        &mut self,
        affects_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !affects_editor {
            return true;
        }
        if let Some(message) = self.editor_draft_blocker(cx) {
            self.reveal_editor(message, cx);
            return false;
        }
        self.close_editor(window, cx)
    }

    pub(super) fn confirm_history_action(
        &mut self,
        kind: HistoryConfirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation != Some(kind)
            || !self.confirmation_active(kind)
            || self.confirmation_pending(kind)
        {
            return;
        }
        match kind {
            HistoryConfirmation::Group(_) => self.delete_group(cx),
            HistoryConfirmation::Selection => {
                let affects_editor = self
                    .editor
                    .id
                    .is_some_and(|id| self.selected_ids.contains(&id));
                if !self.close_history_editor_for_action(affects_editor, window, cx) {
                    return;
                }
                self.delete_selected(cx);
            }
            HistoryConfirmation::Item(id) => {
                if !self.close_history_editor_for_action(self.editor.id == Some(id), window, cx) {
                    return;
                }
                self.send(Command::Delete(id), cx);
                self.close_history_confirmation(window, cx);
            }
        }
        window.refresh();
    }

    pub(super) fn close_history_confirmation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation.take().is_some() {
            self.confirmation_error = None;
            window.close_dialog(cx);
        }
    }

    pub(super) fn sync_confirmation_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kind) = self.confirmation
            && !self.confirmation_active(kind)
            && !self.confirmation_pending(kind)
        {
            self.close_history_confirmation(window, cx);
        }
    }

    pub(super) fn open_history_confirmation(
        &mut self,
        kind: HistoryConfirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation == Some(kind) || !self.dismiss_note_for_dialog(window, cx) {
            return;
        }
        self.close_history_confirmation(window, cx);
        self.confirmation = Some(kind);
        self.confirmation_error = None;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
        let view = owner.upgrade().expect("history dialog requires its owner");
        let view = view.read(cx);
        let language = view.language;
        let pending = view.confirmation_pending(kind);
        let error = view.confirmation_error.clone();
        let (title, prompt, confirm_label) = match kind {
            HistoryConfirmation::Group(id) => {
                let (name, count) = view.groups.iter()
                    .find(|group| group.id == id)
                    .map(|group| (group.name.as_str(), group.item_count))
                    .unwrap_or((tr(language, "该分组", "this group"), 0));
                let prompt = if language == LanguagePreference::English {
                    format!("Delete “{name}”? {count} items will move to the default group.")
                } else {
                    format!("删除「{name}」？{count} 条记录将移到默认分组。")
                };
                (tr(language, "删除分组", "Delete group"), prompt,
                 tr(language, "保留记录并删除分组", "Delete group and keep items").to_owned())
            }
            HistoryConfirmation::Selection => {
                let count = view.selected_ids.len();
                let prompt = if language == LanguagePreference::English {
                    format!("Delete the {count} selected items? This includes pinned and favorite items and cannot be undone.")
                } else {
                    format!("确定删除选中的 {count} 条记录？包含置顶和收藏，无法撤销。")
                };
                let confirm = if language == LanguagePreference::English {
                    format!("Delete {count} items")
                } else {
                    format!("确认删除 {count} 条")
                };
                (tr(language, "删除选中", "Delete selected"), prompt, confirm)
            }
            HistoryConfirmation::Item(_) => (
                tr(language, "删除记录", "Delete item"),
                tr(
                    language,
                    "确定删除这条记录？无法撤销。",
                    "Delete this item? This cannot be undone.",
                )
                .to_owned(),
                tr(language, "删除", "Delete").to_owned(),
            ),
        };
        let confirm_owner = owner.clone();
        let keyboard_owner = owner.clone();
        let cancel_owner = owner.clone();
        dialog_shell(dialog, cx)
            .title(dialog_title(title, cx))
            .on_ok(move |_, window, cx| {
                let _ = keyboard_owner.update(cx, |view, cx| view.confirm_history_action(kind, window, cx));
                false
            })
            .on_cancel(move |_, _, cx| {
                cancel_owner.update(cx, |view, cx| view.cancel_confirmation(kind, cx)).unwrap_or(true)
            })
            .child(
                dialog_copy()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pt_1()
                    .child(prompt)
                    .when_some(error, |body, error| {
                        body.child(Alert::error("history-confirm-error", error).small())
                    }),
            )
            .footer(dialog_footer(cx)
                .child(Button::new("history-confirm-cancel")
                    .outline()
                    .label(tr(language, "取消", "Cancel"))
                    .disabled(pending)
                    .on_click({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            if owner.update(cx, |view, cx| view.cancel_confirmation(kind, cx)).unwrap_or(false) {
                                window.close_dialog(cx);
                            }
                        }
                    }))
                .child(Button::new("history-confirm-submit")
                    .danger()
                    .label(if pending {
                        match kind {
                            HistoryConfirmation::Group(_) => tr(
                                language,
                                "正在删除分组",
                                "Deleting group",
                            ),
                            HistoryConfirmation::Selection
                            | HistoryConfirmation::Item(_) => {
                                tr(language, "正在删除", "Deleting")
                            }
                        }
                        .to_owned()
                    } else {
                        confirm_label
                    })
                    .disabled(pending)
                    .on_click(move |_, window, cx| {
                        let _ = confirm_owner.update(cx, |view, cx| view.confirm_history_action(kind, window, cx));
                    })))
    });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::HistoryConfirmation;

    #[test]
    fn confirmation_cannot_apply_to_another_group_or_an_empty_selection() {
        let confirmation = HistoryConfirmation::Group(7);
        assert!(confirmation.matches_scope(Some(7), false));
        assert!(!confirmation.matches_scope(Some(8), true));
        assert!(!confirmation.matches_scope(None, true));
        assert!(HistoryConfirmation::Selection.matches_scope(Some(7), true));
        assert!(!HistoryConfirmation::Selection.matches_scope(Some(7), false));
    }
}
