use super::*;

impl ClipboardView {
    pub(super) fn handle_search_input(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.todo_layout.borrow_mut().invalidate_filter();
        }
        if self.todos_open && matches!(event, InputEvent::Change) {
            cx.notify();
            return;
        }
        // Enter on the kept search text leaves the located view and searches again.
        let resume = self.located.is_some()
            && !self.todos_open
            && matches!(event, InputEvent::PressEnter { .. });
        if matches!(event, InputEvent::Change) || resume {
            self.located = None;
            self.locate_id = None;
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
            // Same path as list Enter / card click: paste when a target exists.
            self.activate_row(id, false, window, cx);
        }
    }
}
