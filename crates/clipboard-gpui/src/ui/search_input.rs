use super::*;

impl ClipboardView {
    pub(super) fn handle_search_input(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.todos_open && matches!(event, InputEvent::Change) {
            cx.notify();
            return;
        }
        if matches!(event, InputEvent::Change) {
            self.cancel_sequence(cx);
            self.close_hover_preview("search_changed", cx);
            if matches!(
                self.confirmation,
                Some(HistoryConfirmation::ClearHistory(_))
            ) {
                self.close_history_confirmation(window, cx);
            }
            self.reset_selection();
            self.sync_confirmation_dialog(window, cx);
            self.release_history_drag(cx);
            let reload = self.history.begin_search();
            self.scroll_to_top();
            self.arm_history_query(reload, true, cx);
            cx.notify();
        } else if matches!(event, InputEvent::PressEnter { .. })
            && !self.todos_open
            && !self.batch_mode
            && let Some(id) = self.history.selected
        {
            self.send(
                Command::Copy {
                    id,
                    mode: CopyMode::Original,
                    purpose: CopyPurpose::Copy,
                },
                cx,
            );
        }
    }
}
