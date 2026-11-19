use super::todo_drag::TodoDrag;
use super::*;
use clipboard_core::preferences::{DisplayPreference, TodoPreference};

#[derive(Clone)]
pub(super) struct TodoDragSnapshot {
    pub todo: Todo,
    pub display: DisplayPreference,
    pub language: LanguagePreference,
    pub todo_preference: TodoPreference,
}

pub(super) fn todo_drag_snapshot(view: &ClipboardView, todo: &Todo) -> TodoDragSnapshot {
    TodoDragSnapshot {
        todo: todo.clone(),
        display: view.display.value,
        language: view.language,
        todo_preference: view.todo_preference.value,
    }
}

pub(super) fn todo_empty(message: &str, cx: &App) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .px(px(visual::SPACE_6))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(visual::SPACE_3))
        .child(
            div()
                .size(px(48.))
                .rounded(cx.theme().radius_lg)
                .bg(cx.theme().muted)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(IconName::CircleCheck)
                        .size(px(visual::SPACE_6))
                        .text_color(cx.theme().muted_foreground),
                ),
        )
        .child(
            div()
                .max_w(px(320.))
                .text_size(px(visual::TEXT_BODY))
                .text_center()
                .line_height(relative(1.5))
                .text_color(cx.theme().muted_foreground)
                .child(message.to_owned()),
        )
}

pub(super) fn card_excerpt(text: &str) -> String {
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

pub(super) fn todo_row_surface(
    snapshot: &TodoDragSnapshot,
    preview: bool,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    memo_history_card(snapshot, preview, false, cx)
}

fn memo_history_card(snapshot: &TodoDragSnapshot, preview: bool, selected: bool, cx: &App) -> Div {
    let kind = match snapshot.todo.kind {
        MemoKind::Note => tr(snapshot.language, "笔记", "Note"),
        MemoKind::Todo => tr(snapshot.language, "待办", "To-do"),
    };
    let summary = match todo_meta(snapshot) {
        Some(meta) => format!("{kind} · {meta}"),
        None => kind.to_owned(),
    };
    super::drag_visual::HistoryCardSnapshot {
        is_image: false,
        is_files: false,
        is_pinned: false,
        card_index: 0,
        source_app_name: None,
        source_app_icon: None,
        display: snapshot.display,
        language: snapshot.language,
        summary,
        preview_text: card_excerpt(&snapshot.todo.text),
        highlights: Vec::new(),
        thumbnail_path: None,
        thumbnail: None,
        file_warning: false,
        file_icon: IconName::FileText,
        selected,
    }
    .surface(preview, false, None, cx)
}

impl ClipboardView {
    pub(super) fn render_todo_item(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = self.todo_layout.borrow();
        let Some(&source) = layout.sources.get(index) else {
            return div()
                .h(px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6))
                .into_any_element();
        };
        let todo = self.todos[source].clone();
        let id = todo.id;
        let (from, to) = self.drag_motion.offsets(index, &layout.offsets);
        drop(layout);
        let row = self.render_todo_row(todo, self.todos_pending, window, cx);
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
    }

    pub(super) fn render_todo_row(
        &self,
        todo: Todo,
        pending: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = todo.id;
        let status = todo.status;
        let text = todo.text.clone();
        let remove_owner = cx.entity().downgrade();
        let layout = self.todo_layout.clone();
        let list_surface = self.list_surface.downgrade();
        let entity = cx.entity();
        let menu_owner = entity.clone();
        let snapshot = todo_drag_snapshot(self, &todo);
        let density = self
            .todo_preference
            .value
            .density(self.display.value.card_density);
        let card_spacing = visual::card_spacing(density);
        let card_padding = visual::card_padding(density);
        let measured_size = Rc::new(Cell::new(size(px(0.), px(0.))));
        let drag = TodoDrag {
            id,
            snapshot: snapshot.clone(),
            size: measured_size.clone(),
            native_preview: false,
        };
        let drag_enabled = !pending && !self.todo_reorder_pending && self.todos_open;
        // 操作栏是分组子节点，离开卡片后 group_hover 才会收起。选中不显示按钮。
        let surface = div()
            .group("todo-card")
            .w_full()
            .child(memo_history_card(
                &snapshot,
                false,
                self.selected_todo == Some(id),
                cx,
            ))
            .child(todo_action_bar(
                self,
                id,
                &todo.text,
                pending,
                remove_owner,
                card_padding,
                cx,
            ));
        div()
            .id(("todo-row", id as usize))
            .relative()
            .w_full()
            .px(px(PAGE_PADDING))
            .pt(px(card_spacing))
            .pb(px(card_spacing))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let height = f32::from(bounds.size.height);
                        let mut layout = layout.borrow_mut();
                        if layout.record_height(id, height) {
                            let list_surface = list_surface.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = list_surface.update(cx, |_, cx| cx.notify());
                            });
                        }
                        measured_size.set(bounds.size);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(surface)
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered && !this.card_drag_active(cx) {
                    this.select_todo(id, cx);
                }
                this.set_memo_hover(id, *hovered, window, cx);
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.update_hover_anchor(id, event.position.y, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.prepare_memo_drag(id, &text, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    ClipboardView::open_todo_menu(
                        menu_owner.clone(),
                        id,
                        todo.text.clone(),
                        status,
                        event.position,
                        window,
                        cx,
                    );
                },
            )
            .when(drag_enabled, |row| {
                row.cursor_grab()
                    .on_drag(drag, move |drag, offset, window, cx| {
                        let mut preview = drag.clone();
                        entity.update(cx, |this, cx| {
                            this.close_hover_preview("todo_drag_started", cx);
                            this.todo_drop_target = None;
                            this.todo_move_offsets.clear();
                            this.drag_motion.hovered = None;
                            this.start_card_drag_scroll(cx);
                            preview.native_preview = this.begin_card_drag(
                                external_drag::CardDrag::Todo(Rc::new(drag.clone())),
                                offset,
                                window,
                                cx,
                            );
                            cx.notify();
                        });
                        cx.new(|_| preview)
                    })
            })
            .when(self.todo_drag_source_id() == Some(id), |row| {
                row.opacity(0.)
            })
            .into_any_element()
    }
}

fn todo_action_bar(
    view: &ClipboardView,
    id: i64,
    text: &str,
    pending: bool,
    owner: gpui_kit::WeakEntity<ClipboardView>,
    card_padding: f32,
    cx: &mut Context<ClipboardView>,
) -> impl IntoElement {
    let edit_owner = owner.clone();
    let source = text.to_owned();
    div()
        .id(("todo-actions", id as usize))
        .absolute()
        .top(px(card_padding))
        .right(px(card_padding))
        .h(px(visual::CONTROL_HEIGHT_COMPACT))
        .px(px(visual::SPACE_1 / 2.))
        .flex()
        .items_center()
        .gap(px(visual::SPACE_1))
        .rounded(crate::card_radius::card_radius(cx))
        .overflow_hidden()
        .bg(cx.theme().popover)
        .text_color(cx.theme().popover_foreground)
        .border(px(visual::BORDER_WIDTH))
        .border_color(cx.theme().border)
        .shadow(visual::control_shadow())
        .invisible()
        .when(!view.card_drag_active(cx), |bar| {
            bar.group_hover("todo-card", |style| style.visible())
        })
        .child(
            todo_icon_button(
                ("todo-edit", id as usize),
                IconName::FileText,
                tr(view.language, "编辑", "Edit"),
                pending,
            )
            .on_click(move |_, window, cx| {
                let _ = edit_owner.update(cx, |view, cx| {
                    view.open_todo_editor(Some(id), source.clone(), window, cx);
                });
            }),
        )
        .child(
            todo_icon_button(
                ("todo-remove", id as usize),
                gpui_kit::assets::IconName::Trash,
                tr(view.language, "移除", "Remove"),
                pending,
            )
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |view, cx| view.remove_todo(id, cx));
            }),
        )
}

fn todo_icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    label: impl Into<SharedString>,
    disabled: bool,
) -> Button {
    let label = label.into();
    Button::new(id)
        .ghost()
        .xsmall()
        .h(px(visual::SPACE_6))
        .icon(icon.into().size(px(visual::ICON_SMALL)))
        .tooltip(label.clone())
        .accessibility_label(label)
        .disabled(disabled)
}

/// 元信息：非「待处理」时显示进度文案；时间仍按设置。
fn todo_meta(snapshot: &TodoDragSnapshot) -> Option<String> {
    let preference = snapshot.todo_preference;
    let status = match snapshot.todo.status {
        TodoStatus::Pending => None,
        status => Some(super::todo_notice::todo_status_label(
            snapshot.language,
            status,
        )),
    };
    let now = chrono::Local::now().naive_local();
    let created = preference.show_created.then(|| {
        format_card_time(
            &snapshot.todo.created_at,
            snapshot.display.time_format,
            snapshot.language,
            now,
        )
    });
    let edited = preference.show_edited.then(|| {
        format_card_time(
            &snapshot.todo.updated_at,
            snapshot.display.time_format,
            snapshot.language,
            now,
        )
    });
    let chinese = snapshot.language == LanguagePreference::Chinese;
    let times = match (created, edited) {
        (Some(created), Some(edited)) if chinese => Some(format!("创建 {created} · 编辑 {edited}")),
        (Some(created), Some(edited)) => Some(format!("Created {created} · Edited {edited}")),
        (Some(created), None) if chinese => Some(format!("创建 {created}")),
        (Some(created), None) => Some(format!("Created {created}")),
        (None, Some(edited)) if chinese => Some(format!("编辑 {edited}")),
        (None, Some(edited)) => Some(format!("Edited {edited}")),
        (None, None) => None,
    };
    match (status, times) {
        (Some(status), Some(times)) => Some(format!("{status} · {times}")),
        (Some(status), None) => Some(status.to_owned()),
        (None, Some(times)) => Some(times),
        (None, None) => None,
    }
}
