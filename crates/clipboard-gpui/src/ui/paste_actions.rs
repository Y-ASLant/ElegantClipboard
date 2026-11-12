use super::*;

impl ClipboardView {
    pub(super) fn apply_copied(
        &mut self,
        id: i64,
        purpose: CopyPurpose,
        clipboard_sequence: u32,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let request_id = purpose.request_id();
        let delivery = if let Some(request_id) = request_id {
            let Some(delivery) = self.paste_state.receive(request_id) else {
                let _ = self.service.send(Command::CancelPaste { request_id });
                return false;
            };
            Some((delivery, request_id))
        } else {
            None
        };
        self.set_feedback_message(localize_service_message(self.language, message));
        self.is_error = false;
        if sound::enabled(self.audio.value, Sound::Copy, SoundTiming::AfterSuccess) {
            sound::play(Sound::Copy);
        }
        if let Some(((target, wait), request_id)) = delivery {
            self.return_to_paste_target(
                target,
                clipboard_sequence,
                Some(id),
                request_id,
                wait,
                window,
                cx,
            );
        }
        self.start_copy_flash(purpose, id, cx);
        true
    }

    fn start_copy_flash(&mut self, purpose: CopyPurpose, id: i64, cx: &mut Context<Self>) {
        let Some(token) = copy_flash::record_copy_success(
            &mut self.copy_flash,
            &mut self.copy_flash_token,
            purpose,
            id,
        ) else {
            return;
        };
        self.arm_card_flash(token, cx);
    }

    pub(super) fn start_favorite_flash(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
            return;
        };
        if !copy_flash::shows_favorite_flash(item.is_favorite) {
            return;
        }
        let token = copy_flash::begin_card_flash(
            &mut self.copy_flash,
            &mut self.copy_flash_token,
            id,
            copy_flash::CardFlashKind::Favorite,
        );
        self.arm_card_flash(token, cx);
        cx.notify();
    }

    fn arm_card_flash(&mut self, token: u64, cx: &mut Context<Self>) {
        self.copy_flash_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(copy_flash::FLASH_DURATION)
                .await;
            let _ = view.update(cx, |this, cx| {
                if copy_flash::clear_copy_flash(&mut this.copy_flash, token) {
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn apply_merged(
        &mut self,
        purpose: CopyPurpose,
        clipboard_sequence: u32,
        item_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = if let Some(request_id) = purpose.request_id() {
            let Some(delivery) = self.paste_state.receive(request_id) else {
                let _ = self.service.send(Command::CancelPaste { request_id });
                return false;
            };
            Some((delivery, request_id))
        } else {
            None
        };
        self.batch_pending = false;
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        self.reset_selection();
        if sound::enabled(self.audio.value, Sound::Copy, SoundTiming::AfterSuccess) {
            sound::play(Sound::Copy);
        }
        self.paste_state.finish_merge();
        if let Some(((target, wait), request_id)) = target {
            self.return_to_paste_target(
                target,
                clipboard_sequence,
                None,
                request_id,
                wait,
                window,
                cx,
            );
        } else {
            self.set_feedback_message(if self.language == LanguagePreference::English {
                format!("Merged and copied {item_count} items")
            } else {
                format!("已合并 {item_count} 条记录并复制")
            });
            self.is_error = false;
        }
        true
    }
    pub(super) fn paste_selected(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        let Some(target) = self.paste_state.target() else {
            self.set_feedback_message(tr(
                self.language,
                "请从目标应用按全局快捷键唤出，再使用粘贴",
                "Open the clipboard with the global shortcut from the target app before pasting.",
            )
            .into());
            self.is_error = true;
            cx.notify();
            return;
        };
        if self._tray.is_none() || !paste::is_external_target(window, target) {
            self.paste_state.set_target(None);
            self.set_feedback_message(
                tr(
                    self.language,
                    "原窗口不可用，仍可使用复制后手动粘贴",
                    "The previous window is unavailable; copy and paste manually instead.",
                )
                .into(),
            );
            self.is_error = true;
            cx.notify();
            return;
        }
        if self.paste_state.pending() {
            return;
        }
        let request_id = self
            .paste_state
            .begin_copy(target, self.paste_wait_from_history());
        if !self.send(
            Command::Copy {
                id,
                mode: CopyMode::Original,
                purpose: CopyPurpose::Paste { request_id },
            },
            cx,
        ) {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn copy_or_paste_plain_text(
        &mut self,
        id: i64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(id, CopyMode::PlainText, window, cx);
    }

    pub(super) fn copy_or_paste_path(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.copy_or_paste_generic(id, CopyMode::Paths, window, cx);
    }

    pub(super) fn copy_or_paste_image_file(
        &mut self,
        id: i64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(id, CopyMode::Files, window, cx);
    }

    pub(super) fn copy_or_paste_format(
        &mut self,
        id: i64,
        format: CopyFormat,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(id, CopyMode::from(format), window, cx);
    }

    pub(super) fn copy_or_paste_transform(
        &mut self,
        id: i64,
        transform: TextTransform,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(id, CopyMode::Transform(transform), window, cx);
    }

    /// 复制粘贴通用流程：目标窗口可用时走受控粘贴，否则直接复制。
    fn copy_or_paste_generic(
        &mut self,
        id: i64,
        mode: CopyMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.paste_state.pending() {
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
        if !self.send(Command::Copy { id, mode, purpose }, cx) {
            self.interrupt_paste_except(None);
        }
    }
}
