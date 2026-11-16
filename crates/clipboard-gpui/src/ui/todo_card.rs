use super::todo_drag::TodoDrag;
use super::*;
use clipboard_core::preferences::{DisplayPreference, TodoPreference};
use gpui_kit::component::checkbox::Checkbox;

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
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    todo_card_chrome(snapshot, preview, window, cx, None, false)
}

fn todo_card_chrome(
    snapshot: &TodoDragSnapshot,
    preview: bool,
    window: &mut Window,
    cx: &mut App,
    checkbox: Option<Checkbox>,
    selected: bool,
) -> impl IntoElement {
    let density = snapshot
        .todo_preference
        .density(snapshot.display.card_density);
    let padding = visual::card_padding(density);
    let spacing = visual::card_spacing(density);
    let excerpt = card_excerpt(&snapshot.todo.text);
    let fonts = crate::design_system::active_fonts(cx);
    let id = snapshot.todo.id;
    let background = if preview {
        cx.theme().popover
    } else if selected {
        cx.theme().list_active
    } else {
        cx.theme().colors.list
    };
    let motion = gpui_kit::base::motion::Transition::new(crate::design_system::FAST)
        .easing(cx.theme().motion.easing_move.clone());
    let surface_motion = if cx.theme().colors.list.a < 1. {
        gpui_kit::base::motion::Transition::new(Duration::ZERO)
    } else {
        motion.clone()
    };
    let surface_channel = if cx.theme().is_dark() {
        "todo-surface-dark"
    } else {
        "todo-surface-light"
    };
    let border_channel = if cx.theme().is_dark() {
        "todo-border-dark"
    } else {
        "todo-border-light"
    };
    let background = visual::transition_color(
        ElementId::from((surface_channel, id as usize)),
        background,
        surface_motion.clone(),
        window,
        cx,
    );
    let border = visual::transition_color(
        ElementId::from((border_channel, id as usize)),
        if selected {
            cx.theme().list_active_border
        } else {
            crate::design_system::container_border(cx)
        },
        surface_motion,
        window,
        cx,
    );
    let checkbox = checkbox.unwrap_or_else(|| {
        Checkbox::new(("todo-preview", id as u64))
            .checked(snapshot.todo.status.is_finished())
            .disabled(true)
            .accessibility_label(snapshot.todo.text.clone())
    });
    let mut surface = div()
        .id(("todo-surface", id as usize))
        .group("")
        .relative()
        .w_full()
        .min_w_0()
        .p(px(padding))
        .rounded(crate::card_radius::card_radius(cx))
        .overflow_hidden()
        .border(px(visual::BORDER_WIDTH))
        .border_color(border)
        .bg(background)
        .when(!preview, |surface| {
            surface.group_hover("", |surface| surface.bg(cx.theme().list_hover))
        })
        .flex()
        .flex_col()
        .gap(px(spacing))
        .child(
            div()
                .min_w_0()
                .flex()
                .items_start()
                .gap(px(visual::SPACE_3))
                .child(
                    div()
                        .flex_shrink_0()
                        .h(px(visual::PREVIEW_LINE_HEIGHT))
                        .flex()
                        .items_center()
                        .child(checkbox),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .font_family(crate::fonts::resolve_card_family(&fonts))
                        .text_size(px(f32::from(fonts.card_font_size)))
                        .line_height(relative(visual::COPY_LINE_HEIGHT))
                        .line_clamp(snapshot.display.card_max_lines as usize)
                        .text_ellipsis()
                        .text_color(cx.theme().foreground)
                        .when(snapshot.todo.status.is_finished(), |title| {
                            title.line_through()
                        })
                        .child(if excerpt.is_empty() {
                            tr(snapshot.language, "空白待办", "Empty to-do").to_owned()
                        } else {
                            excerpt
                        }),
                ),
        );
    if let Some(meta) = todo_meta(snapshot) {
        surface = surface.child(
            div()
                .h(px(visual::METADATA_ROW_HEIGHT))
                .text_size(px(visual::TEXT_SMALL))
                .line_height(relative(1.))
                .text_color(cx.theme().muted_foreground)
                .child(meta),
        );
    }
    surface
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = todo.id;
        let status = todo.status;
        let text = todo.text.clone();
        let owner = cx.entity().downgrade();
        let remove_owner = owner.clone();
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
        let checkbox = Checkbox::new(("todo", id as u64))
            .checked(status.is_finished())
            .disabled(pending)
            .accessibility_label(format!(
                "{} · {}",
                text,
                super::todo_notice::todo_status_label(self.language, status)
            ))
            .on_change(move |checked, _, cx| {
                let _ = owner.update(cx, |view, cx| {
                    let next = if *checked {
                        TodoStatus::Done
                    } else {
                        TodoStatus::Pending
                    };
                    view.set_todo_status(id, next, cx);
                });
            });
        let surface = div()
            .w_full()
            .child(todo_card_chrome(
                &snapshot,
                false,
                window,
                cx,
                Some(checkbox),
                self.selected_todo == Some(id),
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
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && !this.card_drag_active(cx) {
                    this.select_todo(id, cx);
                }
            }))
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
        .when(view.selected_todo == Some(id), |bar| bar.visible())
        .group_hover("", |bar| bar.visible())
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
