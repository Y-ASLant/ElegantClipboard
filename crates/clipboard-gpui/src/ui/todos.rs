use super::category_tabs::ToolbarTab;
use super::history_scroll_top::FloatingAction;
use super::todo_drag::TodoDrag;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TodoNotice {
    Added,
    Completed(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeStatus {
    Show,
    Wait,
    Drop,
}

#[derive(Clone, Debug)]
pub(super) struct TodoDraft {
    pub id: Option<i64>,
    pub text: String,
}

impl ClipboardView {
    pub(super) fn select_toolbar_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.toolbar_tabs().get(index).copied() {
            Some(ToolbarTab::Todos) => self.open_todos(window, cx),
            Some(ToolbarTab::Category(filter)) => self.select_category(filter, window, cx),
            None => {}
        }
    }

    pub(super) fn open_todos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.todos_open
            || self.group_delete_pending
            || self.clear_pending
            || self.group_reorder_pending
        {
            return;
        }
        self.release_history_drag(cx);
        self.drag_motion = DragMotionState::default();
        self.cancel_sequence(cx);
        self.close_hover_preview("todos", cx);
        self.close_history_confirmation(window, cx);
        self.group_move_id = None;
        self.reset_selection();
        if self.batch_mode {
            self.exit_batch_mode(cx);
        }
        self.todos_open = true;
        self.scroll_to_top();
        self.load_todos(cx);
        cx.notify();
    }

    pub(super) fn load_todos(&mut self, cx: &mut Context<Self>) {
        if !self.todos_pending && self.send(Command::ListTodos, cx) {
            self.todos_pending = true;
            cx.notify();
        }
    }

    /// 待办编辑器与历史条目编辑器（含主窗口备注对话框）共用 editor 会话，
    /// 浮钮的禁用状态必须与此保持一致，否则表现为点击无反应。
    pub(super) fn todo_editor_unavailable(&self) -> bool {
        self.editor_pending()
            || self.todo_edit_saving
            || self.editor.id.is_some()
            || self.editor_window.is_some()
            || self.todo_draft.is_some()
    }

    pub(super) fn open_todo_editor(
        &mut self,
        id: Option<i64>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.todo_editor_unavailable() {
            return;
        }
        self.todo_draft = Some(TodoDraft { id, text });
        self.editor_error = None;
        self.open_editor_window(window, cx, |view| {
            view.todo_draft.is_some() && view.editor_window.is_none()
        });
    }

    pub(super) fn save_todo_editor(&mut self, text: String, cx: &mut Context<Self>) {
        if self.todo_edit_saving {
            return;
        }
        let Some(draft) = self.todo_draft.clone() else {
            return;
        };
        let adding = draft.id.is_none();
        let command = match draft.id {
            Some(id) => Command::UpdateTodo { id, text },
            None => Command::AddTodo(text),
        };
        if self.send(command, cx) {
            if adding {
                self.todo_notice = Some(TodoNotice::Added);
            }
            self.todo_edit_saving = true;
            self.editor_error = None;
            cx.notify();
        }
    }

    pub(super) fn apply_todos(
        &mut self,
        todos: Vec<Todo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(notice) = self.todo_notice {
            match notice_status(notice, &self.todos, &todos) {
                NoticeStatus::Wait => {}
                NoticeStatus::Drop => self.todo_notice = None,
                NoticeStatus::Show => {
                    self.todo_notice = None;
                    if self.todo_preference.value.toast {
                        show_todo_notice(
                            self,
                            notice_text(notice, &self.todos, &todos, self.language),
                            window,
                            cx,
                        );
                    }
                }
            }
        }
        self.start_todo_slide(&todos, cx);
        if self.todo_reorder_pending {
            self.drag_motion = DragMotionState::default();
        }
        self.todos = todos;
        self.todos_pending = false;
        self.todo_reorder_pending = false;
        self.todo_drop_target = None;
        if self.todo_edit_saving {
            self.todo_edit_saving = false;
            self.close_editor(window, cx);
        }
        true
    }

    pub(super) fn render_todos(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.language;
        let pending = self.todos_pending;
        let query = self.search.read(cx).value().trim().to_owned();
        let todos: Vec<_> = super::todo_drag::visible_todos(self, cx).cloned().collect();
        let offsets = self.todo_row_offsets(cx);
        let viewport = self.drag_viewport.clone();
        div()
            .id("todo-content")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                canvas(
                    move |bounds, _, _| viewport.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<TodoDrag>, _, cx| {
                if this.external_drag.is_none() {
                    let drag = external_drag::CardDrag::Todo(Rc::new(event.drag(cx).clone()));
                    this.update_card_drag_target(&drag, event.event.position, cx);
                }
            }))
            .on_drop(cx.listener(|this, drag: &TodoDrag, _, cx| {
                if this.external_drag.is_none() {
                    this.drop_todo(drag, cx);
                }
            }))
            .child(if todos.is_empty() {
                super::todo_card::todo_empty(
                    if self.todos.is_empty() {
                        tr(language, "还没有待办", "No to-dos yet.")
                    } else if !query.is_empty() {
                        tr(
                            language,
                            "没有匹配的待办，试试其他关键词",
                            "No matching to-dos. Try another search.",
                        )
                    } else {
                        tr(
                            language,
                            "已完成的待办已隐藏。",
                            "Completed to-dos are hidden.",
                        )
                    },
                    cx,
                )
                .into_any_element()
            } else {
                div()
                    .id("todo-list")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.todo_scroll)
                    .vertical_scrollbar(&self.todo_scroll)
                    .pb(px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6))
                    .children(todos.into_iter().enumerate().map(|(index, todo)| {
                        let id = todo.id;
                        let row = self.render_todo_row(todo, pending, window, cx);
                        let (from, to) = self.drag_motion.offsets(index, &offsets);
                        if from != 0. || to != 0. {
                            visual::drag_yield(
                                div().child(row),
                                from,
                                to,
                                self.drag_motion.started,
                                self.drag_motion.progress.clone(),
                                cx,
                            )
                        } else if let Some(offset) = self.todo_move_offsets.get(&id).copied() {
                            visual::reflow(
                                div().child(row),
                                offset,
                                format!("todo-move-{}-{id}", self.todo_motion_revision),
                                cx,
                            )
                        } else {
                            row
                        }
                    }))
                    .into_any_element()
            })
            .child(self.render_floating_action(FloatingAction::AddTodo, cx))
    }
}

fn notice_status(notice: TodoNotice, before: &[Todo], after: &[Todo]) -> NoticeStatus {
    match notice {
        TodoNotice::Added => {
            if after
                .iter()
                .any(|todo| before.iter().all(|old| old.id != todo.id))
            {
                NoticeStatus::Show
            } else {
                NoticeStatus::Wait
            }
        }
        TodoNotice::Completed(id) => {
            let was_open = before.iter().any(|todo| todo.id == id && !todo.completed);
            let is_done = after.iter().any(|todo| todo.id == id && todo.completed);
            if was_open && is_done {
                NoticeStatus::Show
            } else if was_open {
                NoticeStatus::Wait
            } else {
                NoticeStatus::Drop
            }
        }
    }
}

fn notice_text(
    notice: TodoNotice,
    before: &[Todo],
    after: &[Todo],
    language: LanguagePreference,
) -> String {
    let text = match notice {
        TodoNotice::Added => after
            .iter()
            .find(|todo| before.iter().all(|old| old.id != todo.id))
            .map(|todo| todo.text.as_str())
            .unwrap_or(""),
        TodoNotice::Completed(id) => after
            .iter()
            .find(|todo| todo.id == id)
            .map(|todo| todo.text.as_str())
            .unwrap_or(""),
    };
    let excerpt = super::todo_card::card_excerpt(text);
    let excerpt = truncate_excerpt(&excerpt, 40);
    let added = matches!(notice, TodoNotice::Added);
    match (language, added, excerpt.is_empty()) {
        (LanguagePreference::Chinese, true, true) => "已添加待办".to_owned(),
        (LanguagePreference::Chinese, true, false) => format!("已添加：{excerpt}"),
        (LanguagePreference::Chinese, false, true) => "已完成待办".to_owned(),
        (LanguagePreference::Chinese, false, false) => format!("已完成：{excerpt}"),
        (_, true, true) => "To-do added".to_owned(),
        (_, true, false) => format!("Added: {excerpt}"),
        (_, false, true) => "To-do completed".to_owned(),
        (_, false, false) => format!("Completed: {excerpt}"),
    }
}

fn truncate_excerpt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..end])
}

fn show_todo_notice(
    view: &mut ClipboardView,
    message: String,
    window: &mut Window,
    cx: &mut Context<ClipboardView>,
) {
    if let Some(handle) = view.todo_toast
        && handle
            .update(cx, |toast, window, cx| {
                toast.replace(message.clone(), window, cx)
            })
            .is_ok()
    {
        return;
    }
    view.todo_toast = super::todo_toast::open(
        message,
        tr(view.language, "关闭", "Close"),
        view.theme,
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::{NoticeStatus, TodoNotice, notice_status, notice_text};
    use clipboard_core::Todo;
    use clipboard_core::preferences::LanguagePreference;

    fn todo(id: i64, text: &str, completed: bool) -> Todo {
        Todo {
            id,
            text: text.to_owned(),
            completed,
            created_at: "2026-11-10 09:00:00".into(),
            updated_at: "2026-11-10 09:00:00".into(),
        }
    }

    #[test]
    fn notice_waits_for_the_matching_list_and_names_the_change() {
        let before = vec![todo(1, "buy milk", false)];
        assert_eq!(
            notice_status(TodoNotice::Added, &before, &before),
            NoticeStatus::Wait
        );
        let added = vec![
            todo(2, "**write** notes", false),
            todo(1, "buy milk", false),
        ];
        assert_eq!(
            notice_status(TodoNotice::Added, &before, &added),
            NoticeStatus::Show
        );
        assert_eq!(
            notice_text(
                TodoNotice::Added,
                &before,
                &added,
                LanguagePreference::Chinese
            ),
            "已添加：write notes"
        );
        assert_eq!(
            notice_status(TodoNotice::Completed(1), &before, &before),
            NoticeStatus::Wait
        );
        let done = vec![todo(1, "buy milk", true)];
        assert_eq!(
            notice_status(TodoNotice::Completed(1), &before, &done),
            NoticeStatus::Show
        );
        assert_eq!(
            notice_text(
                TodoNotice::Completed(1),
                &before,
                &done,
                LanguagePreference::English
            ),
            "Completed: buy milk"
        );
        assert_eq!(
            notice_status(TodoNotice::Completed(9), &before, &done),
            NoticeStatus::Drop
        );
    }
}
