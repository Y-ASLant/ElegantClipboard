use super::*;

impl ClipboardView {
    pub(super) fn apply_copied(
        &mut self,
        id: i64,
        request_id: Option<u64>,
        clipboard_sequence: u32,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = if let Some(request_id) = request_id {
            let Some(target) = self.paste_pending.get(request_id) else {
                let _ = self.service.send(Command::CancelPaste { request_id });
                return false;
            };
            Some(*target)
        } else {
            None
        };
        self.set_feedback_message(localize_service_message(self.language, message));
        self.is_error = false;
        if sound::enabled(self.audio.value, Sound::Copy, SoundTiming::AfterSuccess) {
            sound::play(Sound::Copy);
        }
        if let Some((target, request_id)) = target.zip(request_id) {
            self.return_to_paste_target(
                target,
                clipboard_sequence,
                Some(id),
                request_id,
                window,
                cx,
            );
        }
        true
    }

    pub(super) fn apply_merged(
        &mut self,
        for_paste: bool,
        request_id: Option<u64>,
        clipboard_sequence: u32,
        item_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target = if for_paste {
            let Some(request_id) = request_id else {
                return false;
            };
            let Some(target) = self.paste_pending.get(request_id).copied() else {
                let _ = self.service.send(Command::CancelPaste { request_id });
                return false;
            };
            Some((target, request_id))
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
        self.batch_paste_pending = None;
        if for_paste {
            if let Some((target, request_id)) = target {
                self.return_to_paste_target(
                    target,
                    clipboard_sequence,
                    None,
                    request_id,
                    window,
                    cx,
                );
            } else {
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Merged and copied {item_count} items; paste manually")
                } else {
                    format!("已合并 {item_count} 条记录并复制，请手动粘贴")
                });
                self.is_error = true;
            }
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
        self.cancel_pending_row_click();
        let Some(target) = self.paste_target else {
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
            self.paste_target = None;
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
        if self.paste_pending.is_some() {
            return;
        }
        let request_id = self.paste_pending.begin(target);
        if self.send(Command::CopyForPaste(id, request_id), cx) {
            self.set_feedback_message(
                tr(
                    self.language,
                    "正在复制并返回原窗口…",
                    "Copying and returning to the previous window…",
                )
                .into(),
            );
            self.is_error = false;
            cx.notify();
        } else {
            self.interrupt_paste_except(None);
        }
    }

    pub(super) fn copy_or_paste_plain_text(
        &mut self,
        id: i64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(
            id,
            Command::CopyPlainText,
            Command::CopyPlainTextForPaste,
            (
                "正在复制纯文本并返回原窗口…",
                "Copying plain text and returning to the previous window…",
            ),
            window,
            cx,
        );
    }

    pub(super) fn copy_or_paste_path(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.copy_or_paste_generic(
            id,
            Command::CopyPath,
            Command::CopyPathForPaste,
            (
                "正在复制路径并返回原窗口…",
                "Copying paths and returning to the previous window…",
            ),
            window,
            cx,
        );
    }

    pub(super) fn copy_or_paste_image_file(
        &mut self,
        id: i64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(
            id,
            Command::CopyAsFile,
            Command::CopyAsFileForPaste,
            (
                "正在复制图片文件并返回原窗口…",
                "Copying the image file and returning to the previous window…",
            ),
            window,
            cx,
        );
    }

    pub(super) fn copy_or_paste_format(
        &mut self,
        id: i64,
        format: CopyFormat,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let label = match format {
            CopyFormat::Html => (
                "正在复制 HTML 并返回原窗口…",
                "Copying HTML and returning to the previous window…",
            ),
            CopyFormat::Rtf => (
                "正在复制 RTF 并返回原窗口…",
                "Copying RTF and returning to the previous window…",
            ),
            CopyFormat::PlainText => (
                "正在复制纯文本并返回原窗口…",
                "Copying plain text and returning to the previous window…",
            ),
        };
        self.copy_or_paste_generic(
            id,
            |id| Command::CopyAsFormat { id, format },
            |id, request_id| Command::CopyAsFormatForPaste {
                id,
                format,
                request_id,
            },
            label,
            window,
            cx,
        );
    }

    pub(super) fn copy_or_paste_transform(
        &mut self,
        id: i64,
        transform: TextTransform,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_generic(
            id,
            |id| Command::CopyTransformed { id, transform },
            |id, request_id| Command::CopyTransformedForPaste {
                id,
                transform,
                request_id,
            },
            (
                "正在复制转换后的文本并返回原窗口…",
                "Copying the transformed text and returning to the previous window…",
            ),
            window,
            cx,
        );
    }

    /// 复制粘贴通用流程：目标窗口可用时走受控粘贴，否则直接复制。
    fn copy_or_paste_generic(
        &mut self,
        id: i64,
        copy: impl Fn(i64) -> Command,
        paste: impl Fn(i64, u64) -> Command,
        paste_message: (&'static str, &'static str),
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_pending_row_click();
        if self.paste_pending.is_some() {
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
            paste(id, request_id)
        } else {
            copy(id)
        };
        if !self.send(command, cx) {
            self.interrupt_paste_except(None);
        } else if target.is_some() {
            self.set_feedback_message(tr(self.language, paste_message.0, paste_message.1).into());
            self.is_error = false;
            cx.notify();
        }
    }
}
