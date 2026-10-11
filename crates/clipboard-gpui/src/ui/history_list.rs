use super::*;

impl ClipboardView {
    pub(super) fn render_history_list(
        &self,
        empty_message: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("history-content")
            .key_context("HistoryList")
            .track_focus(&self.list_focus)
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .on_click(cx.listener(|this, _, window, cx| {
                if this._tray.is_some()
                    && this.settings_window.is_none()
                    && !this.settings_window_opening
                {
                    this.hide_visible_window(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviewSelected, window, cx| {
                if !this.batch_mode
                    && let Some(id) = this.history.selected
                {
                    this.open_preview(id, window, cx);
                }
            }))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<HistoryDrag>, _, cx| {
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
                    let target = if inside {
                        history_drop_row(
                            &this.row_offsets,
                            this.scroll.offset().y,
                            event.bounds.top(),
                            y,
                        )
                        .and_then(|(index, after)| {
                            let id = this.history.items.get(index)?.id;
                            (id != event.drag(cx).id).then_some(DropTarget {
                                id,
                                after,
                                allowed: this.valid_drag(event.drag(cx)),
                            })
                        })
                    } else {
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
                this.drop_history_card(drag, cx);
            }))
            .on_action(cx.listener(|this, _: &Next, _, cx| this.select(1, cx)))
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
                this.select(-this.page_step(), cx);
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                this.select(this.page_step(), cx);
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
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(empty_message),
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
                                |this, visible, _, cx| {
                                    visible.map(|index| this.render_row(index, cx)).collect()
                                },
                            )
                            .track_scroll(&self.scroll)
                            .size_full(),
                        )
                        .vertical_scrollbar(&self.scroll)
                        .when(self.history.items.len() > 8, |container| {
                            container.child(
                                div()
                                    .absolute()
                                    .right(px(20.))
                                    .bottom(px(16.))
                                    .rounded_full()
                                    .shadow_md()
                                    .child(
                                        Button::new("history-scroll-to-top")
                                            .size(px(36.))
                                            .rounded_full()
                                            .label("↑")
                                            .tooltip(tr(self.language, "返回顶部", "Back to top"))
                                            .accessibility_label(tr(
                                                self.language,
                                                "返回顶部",
                                                "Back to top",
                                            ))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.scroll_to_top();
                                                cx.notify();
                                            })),
                                    ),
                            )
                        }),
                )
            })
    }
}
