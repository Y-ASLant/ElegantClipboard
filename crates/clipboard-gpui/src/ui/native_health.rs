use super::*;

impl ClipboardView {
    #[cfg(target_os = "linux")]
    pub(super) fn watch_native_listeners(cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if !view
                    .update(cx, |this, cx| {
                        if this.exiting.get() {
                            return false;
                        }
                        let hotkey = this.hotkey.as_ref().and_then(Hotkey::take_error);
                        let paste = this
                            .paste_hotkeys
                            .as_ref()
                            .and_then(PasteHotkeys::take_error);
                        if hotkey.is_some() {
                            tracing::error!("Global shortcut listener stopped");
                            this.hotkey = None;
                        }
                        if paste.is_some() {
                            this.paste_hotkeys = None;
                            tracing::error!("Paste shortcut listener stopped");
                        }
                        let errors: Vec<String> = hotkey.into_iter().chain(paste).collect();
                        if !errors.is_empty() {
                            this.set_feedback_message(errors.join("\n"));
                            this.is_error = true;
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn outside_click_events(
        receiver: async_channel::Receiver<(i32, i32)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let main_window = window.window_handle();
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(position) = receiver.recv().await {
                if let Err(error) = update_clipboard_window(&view, main_window, cx, |this, window, cx| {
                    this.handle_outside_click(position, window, cx);
                }) {
                    tracing::warn!(%error, "Outside-click window update failed");
                    return;
                }
            }
            let _ = view.update(cx, |this, cx| {
                if !this.exiting.get() && this._outside_click_monitor.is_some() {
                    tracing::error!("Outside-click listener channel closed unexpectedly");
                    this._outside_click_monitor = None;
                    this.set_feedback_message(tr(this.language,
                        "窗口外点击监听已停止；请重启应用以恢复自动隐藏。",
                        "Outside-click monitoring stopped; restart the app to restore automatic hiding.").into());
                    this.is_error = true;
                    cx.notify();
                }
            });
        })
    }
}
