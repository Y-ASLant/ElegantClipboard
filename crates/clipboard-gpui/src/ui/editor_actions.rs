use super::*;

impl ClipboardView {
    pub(super) fn editor_pending(&self) -> bool {
        self.editor_save_pending || self.note_save_pending
    }

    /// 结束当前编辑会话。调用方仍负责关窗口或对话框，并决定焦点回到哪里。
    pub(super) fn release_editor_session(&mut self) {
        self.editor.close();
        self.editor_source_hash = None;
        self.editor_error = None;
    }

    pub(super) fn apply_editor_content(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editor_source_hash.is_none() || !self.editor.apply(id, generation, result) {
            return false;
        }
        cx.notify();
        window.refresh();
        true
    }

    pub(super) fn apply_text_edited(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<bool, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editor.id != Some(id)
            || self.editor.generation != generation
            || !self.editor_save_pending
        {
            return false;
        }
        self.editor_save_pending = false;
        match result {
            Ok(_) => {
                self.close_editor(window, cx);
                self.is_error = false;
            }
            Err(error) => {
                self.editor_error = Some(error.clone());
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "保存编辑失败", "Failed to save changes")
                ));
                self.is_error = true;
                window.refresh();
                cx.notify();
            }
        }
        true
    }

    pub(super) fn apply_note_saved(
        &mut self,
        result: Result<i64, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.note_save_pending || result.as_ref().is_ok_and(|id| self.editor.id != Some(*id)) {
            return false;
        }
        self.note_save_pending = false;
        match result {
            Ok(_) => {
                self.close_editor(window, cx);
                self.is_error = false;
            }
            Err(error) => {
                self.editor_error = Some(error.clone());
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "保存备注失败", "Failed to save note")
                ));
                self.is_error = true;
                window.refresh();
            }
        }
        true
    }

    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.editor_pending() {
            return false;
        }
        if self.editor.id.take().is_some() {
            let content_editor = self.editor_source_hash.is_some();
            self.release_editor_session();
            if let Some(handle) = self.editor_window.take() {
                window.focus(&self.list_focus, cx);
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, editor_window, _| editor_window.remove_window());
                });
            } else if content_editor {
                // 独立窗口可能还没创建；不要顺手关掉主窗口上的其他对话框。
                window.focus(&self.list_focus, cx);
            } else {
                self.note_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                window.close_dialog(cx);
                window.focus(&self.list_focus, cx);
            }
            cx.notify();
        }
        true
    }

    fn save_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_pending() {
            return;
        }
        let Some(id) = self.editor.id else {
            return;
        };
        if self.editor_source_hash.is_some() {
            return;
        }
        self.editor_error = None;
        let note = self.note_input.read(cx).value().to_string();
        if self.send(Command::SetNote { id, note }, cx) {
            self.note_save_pending = true;
        } else {
            self.editor_error = Some(self.message.clone());
        }
        window.refresh();
        cx.notify();
    }

    pub(super) fn save_content_editor(&mut self, new_text: String, cx: &mut Context<Self>) {
        if self.editor_pending() {
            return;
        }
        let Some(id) = self.editor.id else {
            return;
        };
        let Some(expected_hash) = self.editor_source_hash.clone() else {
            return;
        };
        if !matches!(
            self.editor.result,
            Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
        ) {
            return;
        }
        self.editor_error = None;
        if self.send(
            Command::EditText {
                id,
                expected_hash,
                new_text,
                generation: self.editor.generation,
            },
            cx,
        ) {
            self.editor_save_pending = true;
        } else {
            self.editor_error = Some(self.message.clone());
        }
        cx.notify();
    }

    pub(super) fn open_editor(
        &mut self,
        id: i64,
        text: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.id.is_some()
            || window.has_active_dialog(cx)
            || self.batch_pending
            || self.history.loading
        {
            return;
        }
        let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
            return;
        };
        if text && !item.kind().is_some_and(|kind| kind.is_text_like()) {
            return;
        }
        let hash = text.then(|| item.content_hash.clone());
        let note = item.note.clone().unwrap_or_default();
        self.cancel_pending_row_click();
        self.close_hover_preview("editor_opening", cx);
        self.history.selected = Some(id);
        self.group_move_id = None;
        self.editor_source_hash = hash;
        self.editor_error = None;
        let generation = self.editor.open(id);
        if text {
            if !self.send(Command::EditContent { id, generation }, cx) {
                self.editor.apply(id, generation, Err(self.message.clone()));
            }
            self.open_content_editor(id, window, cx);
        } else {
            self.note_input
                .update(cx, |input, cx| input.set_value(note, window, cx));
            self.open_note_dialog(window, cx);
        }
        cx.notify();
    }

    fn open_note_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let entity = owner.upgrade().expect("note dialog requires its owner");
            let view = entity.read(cx);
            let pending = view.editor_pending();
            let language = view.language;
            let note = view.note_input.clone();
            let error = view.editor_error.clone();
            let cancel_owner = owner.clone();
            let cancel_button_owner = owner.clone();
            let submit_owner = owner.clone();
            let keyboard_owner = owner.clone();
            dialog_shell(dialog, cx)
                .w(px(visual::DIALOG_WIDTH_COMPACT))
                .title(dialog_title(tr(language, "编辑备注", "Edit note"), cx))
                .on_ok(move |_, window, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| view.save_note(window, cx));
                    false
                })
                .on_cancel(move |_, window, cx| {
                    cx.stop_propagation();
                    let _ = cancel_owner.update(cx, |view, cx| view.close_editor(window, cx));
                    false
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            Input::new(&note)
                                .text_size(px(visual::TEXT_BODY))
                                .disabled(pending),
                        )
                        .when_some(error, |body, error| {
                            body.child(Alert::error("editor-error", error).small())
                        }),
                )
                .footer(
                    dialog_footer(cx)
                        .child(
                            Button::new("editor-cancel")
                                .outline()
                                .label(tr(language, "取消", "Cancel"))
                                .disabled(pending)
                                .on_click(move |_, window, cx| {
                                    let _ = cancel_button_owner
                                        .update(cx, |view, cx| view.close_editor(window, cx));
                                }),
                        )
                        .child(
                            Button::new("editor-save")
                                .primary()
                                .label(if pending {
                                    tr(language, "保存中…", "Saving…")
                                } else {
                                    tr(language, "保存", "Save")
                                })
                                .disabled(pending)
                                .on_click(move |_, window, cx| {
                                    let _ = submit_owner
                                        .update(cx, |view, cx| view.save_note(window, cx));
                                }),
                        ),
                )
        });
        cx.defer_in(window, |view, window, cx| {
            view.note_input
                .update(cx, |input, cx| input.focus(window, cx));
        });
    }
}
