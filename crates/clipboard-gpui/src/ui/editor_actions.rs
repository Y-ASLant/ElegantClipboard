use super::*;
use gpui_kit::component::notification::Notification;

impl ClipboardView {
    pub(super) fn editor_pending(&self) -> bool {
        self.editor_save_pending || self.note_save_pending
    }

    fn editor_draft_dirty(&self, cx: &App) -> bool {
        self.note_original
            .as_ref()
            .is_some_and(|original| self.note_input.read(cx).text() != original)
            || self
                .editor_input
                .as_ref()
                .is_some_and(|(input, original)| input.read(cx).text() != original)
            || self
                .todo_draft
                .as_ref()
                .is_some_and(|draft| draft.id.is_none() && draft.kind != MemoKind::Todo)
    }

    pub(super) fn editor_draft_blocker(&self, cx: &App) -> Option<&'static str> {
        if self.editor_pending() || self.todo_edit_saving {
            Some(tr(
                self.language,
                "编辑内容仍在保存，请等待保存完成后重试。",
                "Changes are still saving. Please wait and try again.",
            ))
        } else if self.editor_draft_dirty(cx) {
            Some(tr(
                self.language,
                "请先保存或放弃未保存的更改，然后重试。",
                "Save or discard your unsaved changes, then try again.",
            ))
        } else {
            None
        }
    }

    pub(super) fn reveal_editor(&self, message: &'static str, cx: &mut Context<Self>) {
        let handle = self.editor_window.unwrap_or(self.main_window);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                if !native_window::is_window_shown(window)
                    || native_window::is_window_minimized(window)
                {
                    native_window::set_window_visible(window, true);
                }
                native_window::raise_front(window);
                native_window::activate_for_input(window);
                window.push_notification(Notification::warning(message), cx);
            });
        });
    }

    pub(super) fn prepare_app_exit(&self, cx: &mut Context<Self>) -> bool {
        if let Some(message) = self.editor_draft_blocker(cx) {
            self.reveal_editor(message, cx);
            return false;
        }
        true
    }

    // Main-window overlays share a Root with the modal note, not with standalone editors.
    pub(super) fn dismiss_note_for_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.note_original.is_none() {
            return true;
        }
        if let Some(message) = self.editor_draft_blocker(cx) {
            window.push_notification(Notification::warning(message), cx);
            return false;
        }
        self.close_editor(window, cx)
    }

    pub(super) fn request_editor_close(
        &mut self,
        note: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.editor_pending() || self.todo_edit_saving || self.editor_discard_open {
            return false;
        }
        if !self.editor_draft_dirty(cx) {
            return if note {
                self.close_editor(window, cx)
            } else {
                self.close_content_editor_from_window(cx)
            };
        }
        self.editor_discard_open = true;
        let language = self.language;
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let cancel_owner = owner.clone();
            let keep_owner = owner.clone();
            let discard_owner = owner.clone();
            dialog_shell(dialog, cx)
                .centered()
                .title(dialog_title(
                    tr(language, "放弃未保存的更改？", "Discard unsaved changes?"),
                    cx,
                ))
                .on_ok(|_, _, cx| {
                    cx.stop_propagation();
                    false
                })
                .on_cancel(move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = cancel_owner.update(cx, |view, _| view.editor_discard_open = false);
                    true
                })
                .child(dialog_copy().child(tr(
                    language,
                    "如需保留更改，请继续编辑并保存。",
                    "Keep editing and save to keep your changes.",
                )))
                .footer(
                    dialog_footer(cx)
                        .child(
                            Button::new("keep-editing")
                                .outline()
                                .label(tr(language, "继续编辑", "Keep editing"))
                                .on_click(move |_, window, cx| {
                                    let _ = keep_owner
                                        .update(cx, |view, _| view.editor_discard_open = false);
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("discard-edit")
                                .danger()
                                .label(tr(language, "放弃更改", "Discard changes"))
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = discard_owner.update(cx, |view, cx| {
                                        if note {
                                            view.close_editor(window, cx);
                                        } else if view.close_content_editor_from_window(cx) {
                                            window.remove_window();
                                        }
                                    });
                                }),
                        ),
                )
        });
        false
    }

    /// 结束当前编辑会话。调用方仍负责关窗口或对话框，并决定焦点回到哪里。
    pub(super) fn release_editor_session(&mut self) {
        self.editor.close();
        self.editor_input = None;
        self.editor_discard_open = false;
        self.note_original = None;
        self.todo_draft = None;
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
                self.abandon_orphaned_editor(window, cx);
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

    /// Window already gone (release cleared the handle) but a save failed: drop the mutex session.
    pub(super) fn abandon_orphaned_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_window.is_none() {
            let _ = self.close_editor(window, cx);
        }
    }

    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.editor_pending() || self.todo_edit_saving {
            return false;
        }
        if self.editor.id.is_none() && self.todo_draft.is_none() {
            return true;
        }
        let note = self.note_original.is_some();
        if let Some(handle) = self.editor_window {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, editor_window, _| editor_window.remove_window());
            });
        } else {
            self.release_editor_session();
            if note {
                self.note_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                window.close_dialog(cx);
            }
        }
        if !window.has_active_dialog(cx) {
            window.focus(&self.list_focus, cx);
        }
        cx.notify();
        true
    }

    fn save_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_pending() || self.editor_discard_open {
            return;
        }
        let Some(id) = self.editor.id else {
            return;
        };
        if self.editor_source_hash.is_some() {
            return;
        }
        self.editor_error = None;
        let note = self.note_input.read(cx).text().to_string();
        if self.send(Command::SetNote { id, note }, cx) {
            self.note_save_pending = true;
            self.note_input
                .update(cx, |input, cx| input.set_disabled(true, cx));
        } else {
            self.editor_error = Some(self.message.clone());
        }
        window.refresh();
        cx.notify();
    }

    pub(super) fn save_content_editor(&mut self, new_text: String, cx: &mut Context<Self>) {
        if self.editor_pending() || self.editor_discard_open {
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
        if self.todo_editor_unavailable() {
            if let Some(message) = self.editor_draft_blocker(cx) {
                self.reveal_editor(message, cx);
            }
            return;
        }
        if window.has_active_dialog(cx)
            || self.batch_pending
            || self.clear_all_pending
            || self.history.loading
        {
            return;
        }
        let Some(item) = self
            .history
            .items
            .iter()
            .flatten()
            .find(|item| item.id == id)
        else {
            return;
        };
        if text && !item.kind().is_some_and(|kind| kind.is_text_like()) {
            return;
        }
        let hash = text.then(|| item.content_hash.clone());
        let note = item.note.clone().unwrap_or_default();
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
            self.note_original = Some(self.note_input.update(cx, |input, cx| {
                input.set_value(note, window, cx);
                input.text().clone()
            }));
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
                    let _ = cancel_owner
                        .update(cx, |view, cx| view.request_editor_close(true, window, cx));
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
                                    let _ = cancel_button_owner.update(cx, |view, cx| {
                                        view.request_editor_close(true, window, cx)
                                    });
                                }),
                        )
                        .child(
                            Button::new("editor-save")
                                .primary()
                                .label(if pending {
                                    tr(language, "保存中", "Saving")
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
