use super::*;

impl ClipboardView {
    pub(super) fn apply_preview_result(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.preview.apply(id, generation, result) {
            let text = match &self.preview.result {
                Some(Ok(PreviewContent::Text(text))) => Some(text.clone()),
                Some(Ok(PreviewContent::RichText(text))) => Some(text.clone()),
                _ => None,
            };
            if let Some(text) = text {
                self.preview_input.update(cx, |input, cx| {
                    input.set_value(text, window, cx);
                    input.focus(window, cx);
                });
                if self.preview_edit_requested {
                    self.begin_preview_edit(window, cx);
                }
            }
            self.preview_edit_requested = false;
        }
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
        if self.preview.id != Some(id) || self.preview.generation != generation {
            return false;
        }
        self.preview_save_pending = false;
        match result {
            Ok(changed) => {
                self.preview.close();
                self.preview_source_hash = None;
                self.preview_editing = false;
                self.preview_edit_requested = false;
                self.preview_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                window.focus(&self.list_focus, cx);
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
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "保存编辑失败", "Failed to save changes")
                ));
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn open_preview(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.history.selected = Some(id);
        self.preview_source_hash = self
            .history
            .items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.content_hash.clone());
        self.preview_editing = false;
        self.preview_edit_requested = false;
        self.preview_save_pending = false;
        self.image_zoom_percent = 100;
        let generation = self.preview.open(id);
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        // 备注输入同步当前记录的已存值；列表行携带 note（查询列含 note）。
        let note = self
            .history
            .items
            .iter()
            .find(|item| item.id == id)
            .and_then(|item| item.note.clone())
            .unwrap_or_default();
        self.note_input
            .update(cx, |input, cx| input.set_value(note, window, cx));
        self.note_save_pending = false;
        if !self.send(Command::Preview { id, generation }, cx) {
            self.preview
                .apply(id, generation, Err(self.message.clone()));
        }
        cx.notify();
    }

    pub(super) fn open_preview_for_edit(
        &mut self,
        id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_preview(id, window, cx);
        self.preview_edit_requested = true;
    }

    pub(super) fn close_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_save_pending {
            return;
        }
        if self.preview_editing {
            self.preview_editing = false;
            let original = match &self.preview.result {
                Some(Ok(PreviewContent::Text(text) | PreviewContent::RichText(text))) => {
                    Some(text.clone())
                }
                _ => None,
            };
            if let Some(original) = original {
                self.preview_input
                    .update(cx, |input, cx| input.set_value(original, window, cx));
            }
            window.focus(&self.list_focus, cx);
            cx.notify();
            return;
        }
        self.preview.close();
        self.preview_source_hash = None;
        self.preview_edit_requested = false;
        self.image_zoom_percent = 100;
        self.preview_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.note_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.note_save_pending = false;
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    pub(super) fn begin_preview_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_source_hash.is_some()
            && matches!(
                self.preview.result,
                Some(Ok(PreviewContent::Text(_) | PreviewContent::RichText(_)))
            )
        {
            self.preview_editing = true;
            self.preview_input
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    pub(super) fn zoom_image(&mut self, change: i16, cx: &mut Context<Self>) {
        if !matches!(self.preview.result, Some(Ok(PreviewContent::Image(_)))) {
            return;
        }
        let next = (i32::from(self.image_zoom_percent) + i32::from(change)).clamp(50, 400) as u16;
        if next != self.image_zoom_percent {
            self.image_zoom_percent = next;
            cx.notify();
        }
    }
    pub(super) fn save_preview_note(&mut self, cx: &mut Context<Self>) {
        if self.note_save_pending {
            return;
        }
        let Some(id) = self.preview.id else {
            return;
        };
        let note = self.note_input.read(cx).value().to_string();
        if self.send(Command::SetNote { id, note }, cx) {
            self.note_save_pending = true;
            cx.notify();
        }
    }
    pub(super) fn save_preview_edit(&mut self, cx: &mut Context<Self>) {
        if !self.preview_editing || self.preview_save_pending {
            return;
        }
        let (Some(id), Some(expected_hash)) = (self.preview.id, self.preview_source_hash.clone())
        else {
            return;
        };
        let new_text = self.preview_input.read(cx).value().to_string();
        if self.send(
            Command::EditText {
                id,
                expected_hash,
                new_text,
                generation: self.preview.generation,
            },
            cx,
        ) {
            self.preview_save_pending = true;
            cx.notify();
        }
    }
}
