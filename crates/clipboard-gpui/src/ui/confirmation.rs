use super::*;

impl ClipboardView {
    pub(super) fn clear_history(&mut self, cx: &mut Context<Self>) {
        if (!self.clear_confirm_open && !self.skip_clear_confirm.value) || self.clear_pending {
            return;
        }
        if self.send(
            Command::ClearHistory {
                group_id: self.history.group_id(),
                generation: self.history.generation,
            },
            cx,
        ) {
            self.clear_pending = true;
            cx.notify();
        }
    }

    pub(super) fn clear_all_history(&mut self, cx: &mut Context<Self>) {
        if self.clear_all_pending {
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
        match kind {
            HistoryConfirmation::DeleteGroup(id) => {
                self.group_delete_id == Some(id) && self.history.group_id() == Some(id)
            }
            HistoryConfirmation::ClearHistory(group_id) => {
                self.clear_confirm_open && self.history.group_id() == group_id
            }
            HistoryConfirmation::DeleteSelected => self.batch_confirm_open,
        }
    }

    pub(super) fn confirmation_pending(&self, kind: HistoryConfirmation) -> bool {
        match kind {
            HistoryConfirmation::DeleteGroup(_) => self.group_delete_pending,
            HistoryConfirmation::ClearHistory(_) => self.clear_pending,
            HistoryConfirmation::DeleteSelected => self.batch_pending,
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
        match kind {
            HistoryConfirmation::DeleteGroup(_) => self.group_delete_id = None,
            HistoryConfirmation::ClearHistory(_) => self.clear_confirm_open = false,
            HistoryConfirmation::DeleteSelected => self.batch_confirm_open = false,
        }
        self.confirmation = None;
        cx.notify();
        true
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
            HistoryConfirmation::DeleteGroup(_) => self.delete_group(cx),
            HistoryConfirmation::ClearHistory(_) => self.clear_history(cx),
            HistoryConfirmation::DeleteSelected => self.delete_selected(cx),
        }
        if matches!(kind, HistoryConfirmation::ClearHistory(_))
            && !self.clear_pending
            && self.is_error
        {
            self.confirmation_error = Some(self.message.clone());
        }
        window.refresh();
    }

    pub(super) fn sync_confirmation_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(kind) = self.confirmation
            && !self.confirmation_active(kind)
            && !self.confirmation_pending(kind)
        {
            self.confirmation = None;
            window.close_dialog(cx);
        }
    }

    pub(super) fn open_history_confirmation(
        &mut self,
        kind: HistoryConfirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.confirmation == Some(kind) {
            return;
        }
        if self.confirmation.is_some() {
            window.close_dialog(cx);
        }
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
            HistoryConfirmation::DeleteGroup(id) => {
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
            HistoryConfirmation::ClearHistory(group_id) => {
                let name = group_id
                    .and_then(|id| view.groups.iter().find(|group| group.id == id))
                    .map(|group| group.name.as_str())
                    .unwrap_or(tr(language, "默认分组", "Default"));
                let prompt = if language == LanguagePreference::English {
                    format!("Clear unpinned, non-favorite items from “{name}”? Search, favorites, and type filters do not change the scope.")
                } else {
                    format!("清理「{name}」中未置顶且未收藏的记录？搜索、收藏和类型筛选不影响清理范围。")
                };
                (tr(language, "清理历史", "Clear history"), prompt,
                 tr(language, "确认清理", "Clear").to_owned())
            }
            HistoryConfirmation::DeleteSelected => {
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
        };
        let confirm_owner = owner.clone();
        let keyboard_owner = owner.clone();
        let cancel_owner = owner.clone();
        dialog
            .title(title)
            .close_button(false)
            .overlay_closable(false)
            .on_ok(move |_, window, cx| {
                let _ = keyboard_owner.update(cx, |view, cx| view.confirm_history_action(kind, window, cx));
                false
            })
            .on_cancel(move |_, _, cx| {
                cancel_owner.update(cx, |view, cx| view.cancel_confirmation(kind, cx)).unwrap_or(true)
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(prompt)
                    .when(matches!(kind, HistoryConfirmation::ClearHistory(_)), |body| {
                        body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(
                                    language,
                                    "此操作无法撤销；可先导出备份。",
                                    "This cannot be undone. Export a backup first if needed.",
                                )),
                        )
                    })
                    .when_some(error, |body, error| {
                        body.child(Alert::error("history-confirm-error", error).small())
                    }),
            )
            .footer(DialogFooter::new()
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
                            HistoryConfirmation::DeleteGroup(_) => tr(
                                language,
                                "正在删除分组…",
                                "Deleting group…",
                            ),
                            HistoryConfirmation::ClearHistory(_) => {
                                tr(language, "正在清理…", "Clearing…")
                            }
                            HistoryConfirmation::DeleteSelected => {
                                tr(language, "正在删除…", "Deleting…")
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

    pub(super) fn open_clear_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group_save_pending
            || self.group_delete_pending
            || self.clear_pending
            || self.batch_pending
        {
            return;
        }
        self.reset_selection();
        self.group_delete_id = None;
        self.group_editor_open = false;
        self.group_rename_id = None;
        self.group_move_id = None;
        self.preview.close();
        window.focus(&self.list_focus, cx);
        if self.skip_clear_confirm.value {
            self.clear_confirm_open = false;
            self.sync_confirmation_dialog(window, cx);
            self.clear_history(cx);
        } else {
            self.clear_confirm_open = true;
            self.open_history_confirmation(
                HistoryConfirmation::ClearHistory(self.history.group_id()),
                window,
                cx,
            );
        }
        cx.notify();
    }
}
