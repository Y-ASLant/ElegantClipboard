use super::*;
use gpui_kit::component::checkbox::Checkbox;

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

pub(super) fn todo_row(
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
    let heights = view.todo_heights.clone();
    div()
        .group("")
        .relative()
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
                                    view.todo_notice =
                                        checked.then_some(super::todos::TodoNotice::Completed(id));
                                    view.todos_pending = true;
                                    cx.notify();
                                }
                            });
                        }),
                ),
        )
        .child(todo_card(view, &todo, cx))
        .child(
            canvas(
                move |bounds, _, _| {
                    heights
                        .borrow_mut()
                        .insert(id, f32::from(bounds.size.height));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .child(todo_actions(view, id, &text, pending, remove_owner, cx))
}

fn todo_actions(
    view: &ClipboardView,
    id: i64,
    text: &str,
    pending: bool,
    owner: gpui_kit::WeakEntity<ClipboardView>,
    cx: &mut Context<ClipboardView>,
) -> impl IntoElement {
    let padding = visual::card_padding(
        view.todo_preference
            .value
            .density(view.display.card_density),
    );
    let edit_owner = owner.clone();
    let source = text.to_owned();
    div()
        .id(("todo-actions", id as usize))
        .absolute()
        .top(px(padding))
        .right(px(padding))
        .h(px(visual::CONTROL_HEIGHT_COMPACT))
        .px(px(visual::SPACE_1 / 2.))
        .flex()
        .items_center()
        .gap(px(visual::SPACE_1))
        .rounded(cx.theme().radius)
        .bg(cx.theme().popover)
        .text_color(cx.theme().popover_foreground)
        .border(px(visual::BORDER_WIDTH))
        .border_color(cx.theme().border)
        .shadow(visual::control_shadow())
        .invisible()
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
                let _ = owner.update(cx, |view, cx| {
                    if !view.todos_pending && view.send(Command::DeleteTodo(id), cx) {
                        view.todo_notice = None;
                        view.todos_pending = true;
                        cx.notify();
                    }
                });
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
