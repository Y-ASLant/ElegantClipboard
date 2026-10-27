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
            let Some(target) = self.paste_pending.complete(request_id) else {
                return false;
            };
            Some(target)
        } else {
            None
        };
        self.set_feedback_message(localize_service_message(self.language, message));
        self.is_error = false;
        if sound::enabled(self.audio.value, Sound::Copy, SoundTiming::AfterSuccess) {
            sound::play(Sound::Copy);
        }
        if let Some(target) = target {
            self.return_to_paste_target(target, clipboard_sequence, Some(id), window, cx);
        }
        true
    }

    pub(super) fn apply_merged(
        &mut self,
        for_paste: bool,
        clipboard_sequence: u32,
        item_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.batch_pending = false;
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        self.reset_selection();
        if sound::enabled(self.audio.value, Sound::Copy, SoundTiming::AfterSuccess) {
            sound::play(Sound::Copy);
        }
        let target = self.batch_paste_pending.take();
        if for_paste {
            if let Some(target) = target {
                self.return_to_paste_target(target, clipboard_sequence, None, window, cx);
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
            self.paste_pending.take();
        }
    }

    pub(super) fn copy_or_paste_plain_text(
        &mut self,
        id: i64,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.copy_or_paste_variant(
            id,
            (Command::CopyPlainText(id), Command::CopyPlainTextForPaste),
            (
                "正在复制纯文本并返回原窗口…",
                "Copying plain text and returning to the previous window…",
            ),
            window,
            cx,
        );
    }

    pub(super) fn copy_or_paste_path(&mut self, id: i64, window: &Window, cx: &mut Context<Self>) {
        self.copy_or_paste_variant(
            id,
            (Command::CopyPath(id), Command::CopyPathForPaste),
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
        self.copy_or_paste_variant(
            id,
            (Command::CopyAsFile(id), Command::CopyAsFileForPaste),
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

    /// 转换/格式变体的通用复制粘贴（与 copy_or_paste_variant 同构，
    /// 但命令按需构造而非常量元组）。
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
            self.paste_pending.take();
        } else if target.is_some() {
            self.set_feedback_message(tr(self.language, paste_message.0, paste_message.1).into());
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn copy_or_paste_variant(
        &mut self,
        id: i64,
        commands: (Command, fn(i64, u64) -> Command),
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
            commands.1(id, request_id)
        } else {
            commands.0
        };
        if !self.send(command, cx) {
            self.paste_pending.take();
        } else if target.is_some() {
            self.set_feedback_message(tr(self.language, paste_message.0, paste_message.1).into());
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn return_to_paste_target(
        &mut self,
        target: (isize, u32),
        clipboard_sequence: u32,
        pasted_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_delivery_task = None;
        if !paste::is_external_target(window, target) {
            self.set_feedback_message(
                tr(
                    self.language,
                    "目标窗口已变化，内容已复制，请手动粘贴",
                    "The target window changed. The content was copied; paste it manually.",
                )
                .into(),
            );
            self.is_error = true;
            return;
        }
        let pending =
            match paste::PendingPaste::begin(target, clipboard_sequence, self.paste_key.value) {
                Ok(pending) => pending,
                Err(error) => {
                    self.set_feedback_message(localize_service_message(
                        self.language,
                        error.to_string(),
                    ));
                    self.is_error = true;
                    return;
                }
            };
        let hide_window = self.paste_close_window.value && native_window::is_window_shown(window);
        if hide_window {
            self.hide_window(window, cx);
        }
        if sound::enabled(self.audio.value, Sound::Paste, SoundTiming::Immediate) {
            sound::play(Sound::Paste);
        }
        let main_window = window.window_handle();
        self.paste_delivery_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(paste::POLL_INTERVAL).await;
                let finished =
                    update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                        match pending.try_send() {
                            Ok(false) => return false,
                            Ok(true) => {
                                if sound::enabled(
                                    this.audio.value,
                                    Sound::Paste,
                                    SoundTiming::AfterSuccess,
                                ) {
                                    sound::play(Sound::Paste);
                                }
                                this.set_feedback_message(
                                    tr(
                                        this.language,
                                        "已发送粘贴快捷键，请检查目标应用",
                                        "Paste shortcut sent; check the target app.",
                                    )
                                    .into(),
                                );
                                this.is_error = false;
                                // 粘贴已送达：按偏好写回用户原剪贴板（决策 0003）。
                                this.send(Command::RestoreStashedClipboard, cx);
                                if let Some(id) = pasted_id.filter(|_| this.paste_move_to_top.value)
                                {
                                    this.send(Command::BumpToTop(id), cx);
                                }
                                // 序列粘贴：送达一条即记录，等推进键触发下一条（决策 0004）。
                                this.sequence_paste_delivered(cx);
                            }
                            Err(error) => {
                                if hide_window {
                                    this.show_window(window, cx);
                                }
                                this.set_feedback_message(localize_service_message(
                                    this.language,
                                    error.to_string(),
                                ));
                                this.is_error = true;
                            }
                        }
                        cx.notify();
                        true
                    });
                if !matches!(finished, Ok(false)) {
                    break;
                }
            }
        }));
    }
}
