use super::*;

impl ClipboardView {
    pub(super) fn render_history_list(
        &self,
        empty_message: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let viewport = self.drag_viewport.clone();
        div()
            .id("history-content")
            .relative()
            .child(
                canvas(
                    move |bounds, _, _| viewport.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.drag_release_pending = false;
                }),
            )
            .key_context("HistoryList")
            .track_focus(&self.list_focus)
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
                    if this.external_drag.is_some() {
                        return;
                    }
                    let y = event.event.position.y;
                    let inside = event.bounds.contains(&event.event.position);
                    let direction = if !inside {
                        0
                    } else if y < event.bounds.top() + px(visual::DRAG_EDGE_ZONE) {
                        -1
                    } else if y > event.bounds.bottom() - px(visual::DRAG_EDGE_ZONE) {
                        1
                    } else {
                        0
                    };
                    let target = if inside && this.valid_drag(event.drag(cx)) {
                        this.history_drag_target(event.drag(cx), event.bounds.top(), y)
                    } else {
                        this.drag_motion.hovered = None;
                        None
                    };
                    if this.history_drag_direction != direction || this.drop_target != target {
                        this.history_drag_direction = direction;
                        this.drop_target = target;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, drag: &HistoryDrag, _, cx| {
                if this.external_drag.is_none() {
                    this.drop_history_card(drag, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Next, _, cx| {
                this.select(1, ScrollStrategy::Nearest, cx);
            }))
            .on_action(cx.listener(|this, _: &Previous, window, cx| {
                this.select_previous_or_focus_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousCategory, window, cx| {
                this.select_adjacent_category(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextCategory, window, cx| {
                this.select_adjacent_category(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousGroup, window, cx| {
                this.select_adjacent_group(-1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextGroup, window, cx| {
                this.select_adjacent_group(1, window, cx);
            }))
            .on_action(cx.listener(|this, _: &First, _, cx| {
                this.select_index(0, ScrollStrategy::Top, cx);
            }))
            .on_action(cx.listener(|this, _: &Last, _, cx| {
                this.select_index(usize::MAX, ScrollStrategy::Bottom, cx);
            }))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                this.select(-this.page_step(), ScrollStrategy::Top, cx);
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                this.select(this.page_step(), ScrollStrategy::Top, cx);
            }))
            .on_action(cx.listener(|this, _: &SelectAllLoaded, window, cx| {
                this.select_all_loaded(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ActivateSelected, window, cx| {
                if !this.batch_mode
                    && let Some(id) = this.history.selected
                {
                    this.activate_row(id, false, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PastePlainTextSelected, window, cx| {
                if !this.batch_mode
                    && let Some(id) = this.history.selected
                {
                    this.copy_or_paste_plain_text(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PasteSelected, window, cx| {
                if !this.batch_mode
                    && let Some(id) = this.history.selected
                {
                    this.paste_selected(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                if !this.batch_mode
                    && let Some(id) = this.history.selected
                {
                    this.send(Command::Delete(id), cx);
                }
            }))
            .when(self.history.items.is_empty(), |container| {
                container.child(
                    div()
                        .size_full()
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
                                    Icon::new(IconName::Copy)
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
                                .child(empty_message),
                        ),
                )
            })
            .when(!self.history.items.is_empty(), |container| {
                container.child(
                    div()
                        .relative()
                        .size_full()
                        .child(
                            v_virtual_list(
                                cx.entity().clone(),
                                "history-virtual-list",
                                self.row_sizes.clone(),
                                |this, visible, window, cx| {
                                    this.update_image_visible_range(visible.clone(), window, cx);
                                    visible
                                        .map(|index| this.render_row(index, window, cx))
                                        .collect()
                                },
                            )
                            .track_scroll(&self.scroll)
                            .size_full(),
                        )
                        .vertical_scrollbar(&self.scroll)
                        .on_scroll(cx.listener(|this, _, _, cx| {
                            this.scroll_to_top_task = None;
                            this.close_hover_preview("history_scrolled", cx);
                            cx.notify();
                        }))
                        .when(self.scroll.offset().y < px(-32.), |container| {
                            container.child(
                                div()
                                    .absolute()
                                    .right(px(visual::SPACE_4 + visual::SPACE_1))
                                    .bottom(px(visual::SPACE_4))
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().popover)
                                    .shadow(visual::control_shadow())
                                    .child(
                                        Button::new("history-scroll-to-top")
                                            .outline()
                                            .size(px(visual::CONTROL_HEIGHT_LARGE))
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().popover)
                                            .border_color(cx.theme().border)
                                            .icon(
                                                Icon::new(
                                                    gpui_kit::assets::IconName::ArrowUpToLine,
                                                )
                                                .size(px(visual::ICON_SIZE)),
                                            )
                                            .tooltip(tr(self.language, "返回顶部", "Back to top"))
                                            .accessibility_label(tr(
                                                self.language,
                                                "返回顶部",
                                                "Back to top",
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.animate_scroll_to_top(cx);
                                                cx.notify();
                                            })),
                                    ),
                            )
                        }),
                )
            })
    }
}
