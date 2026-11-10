use super::category_tabs::ToolbarTab;
use super::history_scroll_top::FloatingAction;
use super::*;
use gpui_kit::component::checkbox::Checkbox;

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
        self.cancel_sequence(cx);
        self.cancel_pending_row_click();
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

    pub(super) fn open_todo_editor(
        &mut self,
        id: Option<i64>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor_pending()
            || self.todo_edit_saving
            || self.editor.id.is_some()
            || self.editor_window.is_some()
            || self.todo_draft.is_some()
        {
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
        self.todos = todos;
        self.todos_pending = false;
        if self.todo_edit_saving {
            self.todo_edit_saving = false;
            self.close_editor(window, cx);
        }
        true
    }

    pub(super) fn render_todos(&self, cx: &mut Context<Self>) -> Div {
        let language = self.language;
        let pending = self.todos_pending;
        let query = self.search.read(cx).value().trim().to_owned();
        let show_completed = self.todo_preference.value.show_completed;
        let todos: Vec<_> = self
            .todos
            .iter()
            .filter(|todo| show_completed || !todo.completed)
            .filter(|todo| {
                query.is_empty() || todo.text.to_lowercase().contains(&query.to_lowercase())
            })
            .cloned()
            .collect();
        let viewport = self.drag_viewport.clone();
        div()
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .px(px(PAGE_PADDING))
                    .py(px(visual::SPACE_2))
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_2))
                    .pb(px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6))
                    .when(todos.is_empty(), |list| {
                        list.child(settings_common::settings_description(
                            if self.todos.is_empty() {
                                tr(
                                    language,
                                    "还没有待办。点添加按钮，用 Markdown 写一条。",
                                    "No to-dos yet. Use Add and write one in Markdown.",
                                )
                            } else if !query.is_empty() {
                                tr(language, "没有匹配的待办", "No matching to-dos.")
                            } else {
                                tr(
                                    language,
                                    "已完成的待办已隐藏。",
                                    "Completed to-dos are hidden.",
                                )
                            },
                            cx,
                        ))
                    })
                    .children(
                        todos
                            .into_iter()
                            .map(|todo| todo_row(self, todo, pending, cx)),
                    ),
            )
            .child(self.render_floating_action(FloatingAction::AddTodo, cx))
    }
}

fn card_excerpt(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.trim_start_matches(|ch: char| "#-*`> ".contains(ch))
                .replace("**", "")
                .replace("__", "")
        })
        .filter(|line| !line.trim().is_empty())
        .unwrap_or_default()
}

fn todo_card(view: &ClipboardView, todo: &Todo, cx: &App) -> Div {
    let fonts = crate::design_system::active_fonts(cx);
    let density = view
        .todo_preference
        .value
        .density(view.display.card_density);
    let excerpt = card_excerpt(&todo.text);
    div()
        .min_w_0()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(visual::card_spacing(density)))
        .child(
            div()
                .min_w_0()
                .font_family(crate::fonts::resolve_card_family(&fonts))
                .text_size(px(f32::from(fonts.card_font_size)))
                .line_height(relative(visual::COPY_LINE_HEIGHT))
                .line_clamp(view.display.card_max_lines as usize)
                .text_ellipsis()
                .text_color(if todo.completed {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().foreground
                })
                .when(todo.completed, |title| title.line_through())
                .child(if excerpt.is_empty() {
                    tr(view.language, "空白待办", "Empty to-do").to_owned()
                } else {
                    excerpt
                }),
        )
        .when_some(todo_times(view, todo), |card, times| {
            card.child(
                div()
                    .h(px(visual::METADATA_ROW_HEIGHT))
                    .flex_none()
                    .text_size(px(visual::TEXT_SMALL))
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(times),
            )
        })
}

fn todo_row(
    view: &ClipboardView,
    todo: Todo,
    pending: bool,
    cx: &mut Context<ClipboardView>,
) -> Div {
    let id = todo.id;
    let completed = todo.completed;
    let text = todo.text.clone();
    let owner = cx.entity().downgrade();
    let remove_owner = owner.clone();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .gap(px(visual::SPACE_3))
        .p(px(visual::card_padding(
            view.todo_preference
                .value
                .density(view.display.card_density),
        )))
        .rounded(crate::card_radius::card_radius(cx))
        .border(px(visual::BORDER_WIDTH))
        .border_color(crate::design_system::container_border(cx))
        .bg(cx.theme().colors.list)
        .child(
            div()
                .flex_shrink_0()
                .h(px(visual::PREVIEW_LINE_HEIGHT))
                .flex()
                .items_center()
                .child(
                    Checkbox::new(("todo", id as u64))
                        .checked(completed)
                        .disabled(pending)
                        .accessibility_label(text.clone())
                        .on_change(move |checked, _, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                if !view.todos_pending
                                    && view.send(
                                        Command::SetTodoCompleted {
                                            id,
                                            completed: *checked,
                                        },
                                        cx,
                                    )
                                {
                                    view.todo_notice = checked.then_some(TodoNotice::Completed(id));
                                    view.todos_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                ),
        )
        .child(todo_card(view, &todo, cx))
        .child(
            div()
                .flex_shrink_0()
                .h(px(visual::PREVIEW_LINE_HEIGHT))
                .flex()
                .items_center()
                .gap(px(visual::SPACE_1))
                .child(
                    Button::new(format!("todo-edit-{id}"))
                        .ghost()
                        .small()
                        .label(tr(view.language, "编辑", "Edit"))
                        .disabled(pending)
                        .on_click({
                            let edit_owner = remove_owner.clone();
                            let source = text;
                            move |_, window, cx| {
                                let _ = edit_owner.update(cx, |view, cx| {
                                    view.open_todo_editor(Some(id), source.clone(), window, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new(format!("todo-remove-{id}"))
                        .ghost()
                        .small()
                        .label(tr(view.language, "移除", "Remove"))
                        .disabled(pending)
                        .on_click(move |_, _, cx| {
                            let _ = remove_owner.update(cx, |view, cx| {
                                if !view.todos_pending && view.send(Command::DeleteTodo(id), cx) {
                                    view.todo_notice = None;
                                    view.todos_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                ),
        )
}

fn todo_times(view: &ClipboardView, todo: &Todo) -> Option<String> {
    let preference = view.todo_preference.value;
    if !preference.show_created && !preference.show_edited {
        return None;
    }
    let now = chrono::Local::now().naive_local();
    let created = preference.show_created.then(|| {
        format_card_time(
            &todo.created_at,
            view.display.time_format,
            view.language,
            now,
        )
    });
    let edited = preference.show_edited.then(|| {
        format_card_time(
            &todo.updated_at,
            view.display.time_format,
            view.language,
            now,
        )
    });
    let chinese = view.language == LanguagePreference::Chinese;
    match (created, edited) {
        (Some(created), Some(edited)) if chinese => Some(format!("创建 {created} · 编辑 {edited}")),
        (Some(created), Some(edited)) => Some(format!("Created {created} · Edited {edited}")),
        (Some(created), None) if chinese => Some(format!("创建 {created}")),
        (Some(created), None) => Some(format!("Created {created}")),
        (None, Some(edited)) if chinese => Some(format!("编辑 {edited}")),
        (None, Some(edited)) => Some(format!("Edited {edited}")),
        (None, None) => None,
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
    let excerpt = card_excerpt(text);
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
