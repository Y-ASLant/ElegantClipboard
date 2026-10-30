use super::*;

impl ClipboardView {
    fn editor_pending(&self) -> bool {
        self.editor_save_pending || self.note_save_pending
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
        if let Some(Ok(PreviewContent::Text(text) | PreviewContent::RichText(text))) =
            &self.editor.result
        {
            self.editor_input.update(cx, |input, cx| {
                input.set_value(text.clone(), window, cx);
                input.focus(window, cx);
            });
        }
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
            Ok(changed) => {
                self.close_editor(window, cx);
                self.set_feedback_message(
                    if changed {
                        tr(
                            self.language,
                            "内容已保存为纯文本",
                            "Content saved as plain text",
                        )
                    } else {
                        tr(self.language, "内容未修改", "Content was not changed")
                    }
                    .into(),
                );
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
                self.set_feedback_message(tr(self.language, "备注已保存", "Note saved").into());
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
            self.editor.close();
            self.editor_source_hash = None;
            self.editor_error = None;
            self.editor_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.note_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            window.close_dialog(cx);
            window.focus(&self.list_focus, cx);
            cx.notify();
        }
        true
    }

    fn save_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_pending() {
            return;
        }
        let Some(id) = self.editor.id else {
            return;
        };
        self.editor_error = None;
        if let Some(expected_hash) = self.editor_source_hash.clone() {
            if !matches!(
                self.editor.result,
                Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
            ) {
                return;
            }
            let new_text = self.editor_input.read(cx).value().to_string();
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
        } else {
            let note = self.note_input.read(cx).value().to_string();
            if self.send(Command::SetNote { id, note }, cx) {
                self.note_save_pending = true;
            } else {
                self.editor_error = Some(self.message.clone());
            }
        }
        window.refresh();
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
        self.editor_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.note_input
            .update(cx, |input, cx| input.set_value(note, window, cx));
        if text && !self.send(Command::EditContent { id, generation }, cx) {
            self.editor.apply(id, generation, Err(self.message.clone()));
        }
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            let entity = owner.upgrade().expect("editor dialog requires its owner");
            let view = entity.read(cx);
            let pending = view.editor_pending();
            let language = view.language;
            let input = view.editor_input.clone();
            let note = view.note_input.clone();
            let ready = !text || matches!(view.editor.result, Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_))));
            let loading = text && view.editor.result.is_none();
            let rich = matches!(view.editor.result, Some(Ok(PreviewContent::RichText(_))));
            let error = view.editor_error.clone().or_else(|| view.editor.result.as_ref().and_then(|result| result.as_ref().err().cloned()));
            let cancel_owner = owner.clone();
            let cancel_button_owner = owner.clone();
            let submit_owner = owner.clone();
            let keyboard_owner = owner.clone();
            let title = if text { tr(language, "编辑内容", "Edit content") } else { tr(language, "编辑备注", "Edit note") };
            dialog
                .w(px(if text { 560. } else { 400. }))
                .p_4()
                .rounded(cx.theme().radius_lg)
                .bg(cx.theme().popover)
                .title(div().pb_3().border_b_1().border_color(cx.theme().border).text_size(px(16.)).font_semibold().child(title))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(move |_, window, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| view.save_editor(window, cx));
                    false
                })
                .on_cancel(move |_, window, cx| {
                    cx.stop_propagation();
                    let _ = cancel_owner.update(cx, |view, cx| view.close_editor(window, cx));
                    false
                })
                .child(
                    div().flex().flex_col().gap_3()
                        .when(text, |body| {
                            body.child(Textarea::new(&input)
                                .h((window.viewport_size().height * 0.42).clamp(px(100.), px(300.)))
                                .text_size(px(14.)).readonly(!ready || pending)
                                .aria_label(tr(language, "编辑文本", "Edit text")))
                        })
                        .when(!text, |body| body.child(Input::new(&note).text_size(px(14.)).disabled(pending)))
                        .when(loading, |body| body.child(div().text_size(px(12.)).text_color(cx.theme().muted_foreground).child(tr(language, "正在加载…", "Loading…"))))
                        .when(rich, |body| body.child(div().text_size(px(12.)).text_color(cx.theme().muted_foreground).child(tr(language, "保存后将变为纯文本，原富文本格式不会保留", "Saving converts this to plain text and removes rich-text formatting."))))
                        .when_some(error, |body, error| body.child(Alert::error("editor-error", error).small())),
                )
                .footer(DialogFooter::new().pt_3().border_t_1().border_color(cx.theme().border)
                    .child(Button::new("editor-cancel").outline().label(tr(language, "取消", "Cancel")).disabled(pending).on_click(move |_, window, cx| {
                        let _ = cancel_button_owner.update(cx, |view, cx| view.close_editor(window, cx));
                    }))
                    .child(Button::new("editor-save").primary().label(if pending { tr(language, "保存中…", "Saving…") } else { tr(language, "保存", "Save") }).disabled(!ready || pending).on_click(move |_, window, cx| {
                        let _ = submit_owner.update(cx, |view, cx| view.save_editor(window, cx));
                    })))
        });
        if !text {
            cx.defer_in(window, |view, window, cx| {
                view.note_input
                    .update(cx, |input, cx| input.focus(window, cx))
            });
        }
        cx.notify();
    }
}
