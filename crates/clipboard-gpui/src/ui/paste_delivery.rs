use super::*;

/// 识别所有会替换剪贴板内容的操作，包含格式与文本转换变体。
pub(super) fn replacement_request(command: &Command) -> Option<Option<u64>> {
    match command {
        Command::Copy(_)
        | Command::CopyPlainText(_)
        | Command::CopyPath(_)
        | Command::CopyAsFile(_)
        | Command::CopyAsFormat { .. }
        | Command::CopyTransformed { .. }
        | Command::MergeCopy(_)
        | Command::ResolveQuickPaste { .. } => Some(None),
        Command::CopyForPaste(_, request_id)
        | Command::CopyPlainTextForPaste(_, request_id)
        | Command::CopyPathForPaste(_, request_id)
        | Command::CopyAsFileForPaste(_, request_id)
        | Command::CopyAsFormatForPaste { request_id, .. }
        | Command::CopyTransformedForPaste { request_id, .. }
        | Command::MergeForPaste { request_id, .. } => Some(Some(*request_id)),
        _ => None,
    }
}

impl ClipboardView {
    /// 替换剪贴板前先取消旧投递及其快照，新分配的请求保留到自己的投递结束。
    pub(super) fn interrupt_paste_except(&mut self, keep_request: Option<u64>) {
        self.paste_delivery_task = None;
        if let Some(request_id) = self.paste_pending.request_id()
            && Some(request_id) != keep_request
        {
            self.paste_pending.take();
            let _ = self.service.send(Command::CancelPaste { request_id });
        }
        if self.batch_paste_pending.take().is_some() {
            self.batch_pending = false;
        }
        if self.sequence_queue.take().is_some() {
            self.reset_selection();
        }
    }

    pub(super) fn return_to_paste_target(
        &mut self,
        target: (isize, u32),
        clipboard_sequence: u32,
        pasted_id: Option<i64>,
        request_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.paste_delivery_task = None;
        if !paste::is_external_target(window, target) {
            self.interrupt_paste_except(None);
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
                    self.interrupt_paste_except(None);
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
                        if this.paste_pending.get(request_id).is_none() {
                            return true;
                        }
                        match pending.try_send() {
                            Ok(false) => return false,
                            Ok(true) => {
                                this.paste_pending.complete(request_id);
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
                                this.send(
                                    Command::RestoreStashedClipboard {
                                        request_id,
                                        clipboard_sequence,
                                    },
                                    cx,
                                );
                                if let Some(id) = pasted_id.filter(|_| this.paste_move_to_top.value)
                                {
                                    this.send(Command::BumpToTop(id), cx);
                                }
                                this.sequence_paste_delivered(request_id, cx);
                                if this.sequence_queue.is_none() {
                                    this.paste_target = None;
                                }
                            }
                            Err(error) => {
                                this.interrupt_paste_except(None);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn format_and_transform_commands_replace_pending_delivery_with_their_own_request() {
        let commands = [
            (
                Command::CopyAsFormat {
                    id: 1,
                    format: CopyFormat::Html,
                },
                None,
            ),
            (
                Command::CopyTransformed {
                    id: 1,
                    transform: TextTransform::Trim,
                },
                None,
            ),
            (
                Command::CopyAsFormatForPaste {
                    id: 1,
                    format: CopyFormat::Rtf,
                    request_id: 9,
                },
                Some(9),
            ),
            (
                Command::CopyTransformedForPaste {
                    id: 1,
                    transform: TextTransform::Lowercase,
                    request_id: 10,
                },
                Some(10),
            ),
            (
                Command::MergeForPaste {
                    ids: vec![1, 2],
                    request_id: 11,
                },
                Some(11),
            ),
        ];
        let mut pending = paste_requests::PendingRequest::default();
        for (command, expected) in commands {
            let old_request = pending.begin((1, 2));
            let keep = replacement_request(&command).expect("clipboard replacement");
            assert_eq!(keep, expected);
            if pending.request_id() != keep {
                pending.take();
            }
            assert_eq!(pending.complete(old_request), None);
        }
        assert_eq!(
            replacement_request(&Command::CancelPaste { request_id: 1 }),
            None
        );
        assert_eq!(
            replacement_request(&Command::RestoreStashedClipboard {
                request_id: 1,
                clipboard_sequence: 2
            }),
            None
        );
    }
}
