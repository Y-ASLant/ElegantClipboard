use super::category_tabs::ToolbarTab;
use super::history_scroll_top::FloatingAction;
use super::todo_drag::TodoDrag;
use super::*;
use gpui_kit::component::menu::PopupMenuItem;

pub(super) use super::todo_notice::TodoNotice;
use super::todo_notice::{
    NoticeStatus, notice_status, notice_text, show_todo_notice, todo_status_label,
};

#[derive(Clone, Debug)]
pub(super) struct TodoDraft {
    pub id: Option<i64>,
    pub text: String,
    pub kind: MemoKind,
}

impl ClipboardView {
    pub(super) fn select_toolbar_tab(
        &mut self,
        tab: ToolbarTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match tab {
            ToolbarTab::Todos => self.open_todos(window, cx),
            ToolbarTab::Category(filter) => self.select_category(filter, window, cx),
        }
    }

    pub(super) fn open_todos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.todo_preference.value.enabled
            || self.todos_open
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
        self.located = None;
        self.locate_id = None;
        self.located_matches.clear();
        self.reset_selection();
        if self.batch_mode {
            self.exit_batch_mode(cx);
        }
        self.todos_open = true;
        self.selected_todo = None;
        self.sync_search_prompt(window, cx);
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
    pub(super) fn sync_search_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if self.todos_open {
            tr(self.language, "搜索备忘录…", "Search memos…")
        } else {
            tr(self.language, "搜索剪贴板历史…", "Search history…")
        };
        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    pub(super) fn select_todo(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.selected_todo == Some(id) {
            return;
        }
        self.selected_todo = Some(id);
        cx.notify();
    }

    pub(super) fn select_adjacent_todo(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.todo_layout.borrow().ids.clone();
        if ids.is_empty() {
            return;
        }
        let current = self
            .selected_todo
            .and_then(|id| ids.iter().position(|item| *item == id));
        if direction < 0 && current == Some(0) {
            self.search
                .update(cx, |search, cx| search.focus(window, cx));
            return;
        }
        let next = match current {
            Some(index) => index.saturating_add_signed(direction).min(ids.len() - 1),
            None => usize::from(direction > 0).saturating_mul(ids.len() - 1),
        };
        self.selected_todo = Some(ids[next]);
        self.todo_scroll
            .scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub(super) fn edit_selected_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_todo else {
            return;
        };
        let Some(text) = self
            .todos
            .iter()
            .find(|todo| todo.id == id)
            .map(|todo| todo.text.clone())
        else {
            return;
        };
        self.open_todo_editor(Some(id), text, window, cx);
    }

    pub(super) fn remove_todo(&mut self, id: i64, cx: &mut Context<Self>) {
        if !self.todos_pending && self.send(Command::DeleteTodo(id), cx) {
            self.todo_notice = None;
            self.todos_pending = true;
            cx.notify();
        }
    }

    pub(super) fn remove_selected_todo(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_todo {
            self.remove_todo(id, cx);
        }
    }

    pub(super) fn open_todo_menu(
        entity: Entity<Self>,
        id: i64,
        text: String,
        status: TodoStatus,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (pending, language) = entity.update(cx, |view, cx| {
            view.selected_todo = Some(id);
            cx.notify();
            (
                view.todos_pending || view.todo_editor_unavailable(),
                view.language,
            )
        });
        menu_window::HistoryMenuWindow::open(
            position,
            window,
            cx,
            entity.clone(),
            move |menu, _, _| {
                let mut menu = menu.min_w(px(180.)).max_w(px(280.));
                for next in [
                    TodoStatus::Pending,
                    TodoStatus::InProgress,
                    TodoStatus::Done,
                    TodoStatus::Closed,
                ] {
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(todo_status_label(language, next))
                            .disabled(pending || status == next)
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |view, cx| {
                                    view.set_todo_status(id, next, cx);
                                });
                            }),
                    );
                }
                menu.separator()
                    .item(
                        PopupMenuItem::new(tr(language, "编辑", "Edit"))
                            .icon(Icon::new(IconName::FileText).size(px(visual::ICON_SIZE)))
                            .disabled(pending)
                            .on_click({
                                let entity = entity.clone();
                                let text = text.clone();
                                move |_, window, cx| {
                                    entity.update(cx, |view, cx| {
                                        view.open_todo_editor(Some(id), text.clone(), window, cx);
                                    });
                                }
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(tr(language, "移除", "Remove"))
                            .icon(
                                Icon::new(gpui_kit::assets::IconName::Trash)
                                    .size(px(visual::ICON_SIZE)),
                            )
                            .disabled(pending)
                            .on_click({
                                let entity = entity.clone();
                                move |_, _, cx| {
                                    entity.update(cx, |view, cx| view.remove_todo(id, cx));
                                }
                            }),
                    )
            },
        );
    }

    pub(super) fn set_todo_status(&mut self, id: i64, status: TodoStatus, cx: &mut Context<Self>) {
        if !self.todos_pending && self.send(Command::SetTodoStatus { id, status }, cx) {
            if status.is_finished() {
                self.todo_notice = Some(TodoNotice::StatusSet { id, status });
            } else {
                self.todo_notice = None;
            }
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
        let kind = id
            .and_then(|id| self.todos.iter().find(|todo| todo.id == id))
            .map(|todo| todo.kind)
            .unwrap_or(MemoKind::Todo);
        self.todo_draft = Some(TodoDraft { id, text, kind });
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
            None => Command::AddTodo {
                text,
                kind: draft.kind,
            },
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
        if self.todos != todos {
            self.todo_layout
                .borrow_mut()
                .retain_measurements(&self.todos, &todos);
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

    pub(super) fn render_todos(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let empty = self.todo_layout.borrow().ids.is_empty();
        let has_query = !self.search.read(cx).value().trim().is_empty();
        let measure_index = self
            .todo_layout
            .borrow()
            .offsets
            .partition_point(|offset| *offset <= -self.todo_scroll.offset().y)
            .saturating_sub(1);
        let viewport = self.drag_viewport.clone();
        div()
            .id("todo-content")
            .key_context("TodoList")
            .track_focus(&self.list_focus)
            .on_action(cx.listener(|this, _: &NextTodo, window, cx| {
                this.select_adjacent_todo(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousTodo, window, cx| {
                this.select_adjacent_todo(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &EditSelectedTodo, window, cx| {
                this.edit_selected_todo(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RemoveSelectedTodo, _, cx| {
                this.remove_selected_todo(cx);
            }))
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
            .child(if empty {
                super::todo_card::todo_empty(
                    if self.todos.is_empty() {
                        tr(language, "还没有备忘录", "No memos yet.")
                    } else if has_query {
                        tr(
                            language,
                            "没有匹配的备忘录，试试其他关键词",
                            "No matching memos. Try another keyword.",
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
                    .relative()
                    .child(
                        v_virtual_list(
                            cx.entity(),
                            "todo-virtual-list",
                            self.todo_layout.borrow().sizes.clone(),
                            |this, visible, window, cx| {
                                visible
                                    .map(|index| this.render_todo_item(index, window, cx))
                                    .collect()
                            },
                        )
                        .track_scroll(&self.todo_scroll)
                        .with_item_to_measure_index(measure_index)
                        .size_full(),
                    )
                    .child(
                        div()
                            .id("todo-scrollbar-rail")
                            .absolute()
                            .right_0()
                            .top_0()
                            .bottom_0()
                            .w(px(visual::SCROLLBAR_RAIL_WIDTH))
                            .vertical_scrollbar(&self.todo_scroll),
                    )
                    .into_any_element()
            })
            .child(self.render_floating_action(FloatingAction::AddTodo, cx))
    }
}
