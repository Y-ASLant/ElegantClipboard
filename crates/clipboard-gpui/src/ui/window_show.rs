use super::*;
use window_lifecycle::log_window_geometry;

impl ClipboardView {
    pub(super) fn show_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log_window_geometry(window, cx, "main_show_requested");
        self.window_show_task = None;
        let interrupted = self.paste_state.show();
        self.apply_paste_interruption(interrupted);
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
                if initialized.is_ok()
                    && let Ok(Some(hwnd)) =
                        update_clipboard_window(&view, main, cx, |_, window, _| {
                            native_window::window_id(window)
                        })
                {
                    let _ = crate::native::preview::hide_from_taskbar(hwnd);
                }
                let position_request =
                    update_clipboard_window(&view, main, cx, |this, window, _| {
                        initialized?;
                        this.initial_window_bounds = None;
                        position::prepare_window_position(window, this.window_position.value)
                    });
                let Ok(position_request) = position_request else {
                    return;
                };
                let result = match position_request {
                    Ok(Some(position)) => position.apply(),
                    Ok(None) => Ok(false),
                    Err(error) => Err(error),
                };
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
                #[cfg(target_os = "macos")]
                native_window::set_window_visible(window, true);
                #[cfg(target_os = "linux")]
                native_window::show_without_activation(window);
            }
            let result = position::position_window(window, self.window_position.value);
            self.finish_show_window(result, window, cx);
        }
    }

    fn finish_show_window(
        &mut self,
        positioning: anyhow::Result<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match positioning {
            Err(error) => {
                tracing::error!(%error, "Main window positioning failed");
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "无法定位窗口", "Failed to position window")
                ));
                self.is_error = true;
            }
            Ok(false) => {}
            Ok(true) => {
                // Auto-fit must not overwrite the saved preferred size.
                self.suppress_window_size_persist = true;
                self.window_size_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;
                    let _ = view.update(cx, |this, _| {
                        this.suppress_window_size_persist = false;
                    });
                }));
            }
        }
        #[cfg(target_os = "macos")]
        native_window::set_window_visible(window, true);
        #[cfg(target_os = "linux")]
        native_window::show_without_activation(window);
        if let Some(editor) = self.editor_window {
            native_window::release_above(window);
            let _ = editor.update(cx, |_, editor_window, _| {
                native_window::raise_front(editor_window);
            });
        }
        #[cfg(target_os = "macos")]
        if self.editor_window.is_none() {
            native_window::raise_above(window);
        }
        log_window_geometry(window, cx, "main_shown");
        if self.onboarding_visible() {
            window.focus(&self.list_focus, cx);
            // Shown without activation so a later paste keeps the target app.
            // Onboarding has to take keys itself: tabs, switches and Enter.
            native_window::activate_for_input(window);
        } else {
            if self.search_auto_clear.value && !self.search.read(cx).value().is_empty() {
                self.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.release_history_drag(cx);
                self.located = None;
                let reload = self.history.begin_search();
                self.scroll_to_top();
                self.arm_history_query(reload, false, cx);
            }
            if self.editor.id.is_none() && self.editor_window.is_none() {
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
