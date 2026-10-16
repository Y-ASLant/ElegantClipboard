use super::*;

impl ClipboardView {
    pub(super) fn handle_search_input(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.cancel_pending_row_click();
            self.close_hover_preview(cx);
            if matches!(
                self.confirmation,
                Some(HistoryConfirmation::ClearHistory(_))
            ) {
                self.close_history_confirmation(window, cx);
            }
            self.reset_selection();
            self.sync_confirmation_dialog(window, cx);
            self.history.begin_search();
            self.scroll_to_top();
            self.search_task = Some(cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                let _ = view.update(cx, |this, cx| this.query(cx));
            }));
            cx.notify();
        } else if matches!(event, InputEvent::Focus) {
            self.cancel_pending_row_click();
        } else if matches!(event, InputEvent::PressEnter { .. })
            && !self.batch_mode
            && let Some(id) = self.history.selected
        {
            self.send(Command::Copy(id), cx);
        }
    }
}
