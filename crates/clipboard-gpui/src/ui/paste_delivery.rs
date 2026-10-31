use super::*;

impl ClipboardView {
    /// 替换剪贴板前先取消旧投递及其快照，新分配的请求保留到自己的投递结束。
    pub(super) fn interrupt_paste_except(&mut self, keep_request: Option<u64>) {
        let interrupted = self.paste_state.interrupt(keep_request);
        self.apply_paste_interruption(interrupted);
    }

    pub(super) fn apply_paste_interruption(&mut self, interrupted: paste_coordinator::Interrupted) {
        self.paste_delivery_task = None;
        if let Some(request_id) = interrupted.request_id {
            let _ = self.service.send(Command::CancelPaste { request_id });
        }
        if interrupted.merge {
            self.batch_pending = false;
        }
        if interrupted.sequence {
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
                        if !this.paste_state.delivering(request_id) {
                            return true;
                        }
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
