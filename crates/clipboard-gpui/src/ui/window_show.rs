use super::*;
use window_lifecycle::log_window_geometry;

impl ClipboardView {
    pub(super) fn show_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log_window_geometry(window, cx, "main_show_requested");
        self.window_show_task = None;
        self.interrupt_paste_except(None);
        self.quick_paste_pending.take();
        self.image_release_task = None;

        #[cfg(windows)]
        {
            let request =
                match native_window::prepare_window_show(window, self.initial_window_bounds) {
                    Ok(request) => request,
                    Err(error) => {
                        self.finish_show_window(Err(error), window, cx);
                        return;
                    }
                };
            let main = window.window_handle();
            self.window_show_task = Some(cx.spawn(async move |view, cx| {
                // Resize, move, show and activate outside GPUI borrows: all can
                // synchronously dispatch callbacks, including on a different display.
                let initialized = request.initialize();
                let prepared = update_clipboard_window(&view, main, cx, |this, window, _| {
                    initialized.and_then(|()| {
                        this.initial_window_bounds = None;
                        position::prepare_window_position(window, this.window_position.value)
                    })
                });
                let Ok(prepared) = prepared else { return };
                let result = prepared
                    .and_then(|position| position.map_or(Ok(()), |position| position.apply()));
                request.show();
                let _ = update_clipboard_window(&view, main, cx, |this, window, cx| {
                    this.window_show_task = None;
                    this.finish_show_window(result, window, cx);
                });
            }));
        }

        #[cfg(not(windows))]
        {
            // Preserve native restore handling on Unix; Windows updates the hidden
            // window's restore placement before restoring it instead.
            if native_window::is_window_minimized(window) {
                native_window::set_window_visible(window, true);
            }
            let result = position::position_window(window, self.window_position.value);
            self.finish_show_window(result, window, cx);
        }
    }

    fn finish_show_window(
        &mut self,
        positioning: anyhow::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = positioning {
            tracing::error!(%error, "Main window positioning failed");
            self.set_feedback_message(format!(
                "{}: {error}",
                tr(self.language, "无法定位窗口", "Failed to position window")
            ));
            self.is_error = true;
        }
        #[cfg(not(windows))]
        native_window::set_window_visible(window, true);
        log_window_geometry(window, cx, "main_shown");
        if self.onboarding_visible() {
            window.focus(&self.list_focus, cx);
        } else {
            if self.search_auto_clear.value && !self.search.read(cx).value().is_empty() {
                self.search_task = None;
                self.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.history.begin_search();
                self.scroll_to_top();
                self.query(cx);
            }
            if self.editor.id.is_none() {
                if self.search_auto_focus.value {
                    self.search.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.list_focus, cx);
                }
            }
        }
        cx.notify();
    }
}
