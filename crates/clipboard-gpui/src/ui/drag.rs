use super::*;

impl ClipboardView {
    pub(super) fn apply_reordered(
        &mut self,
        from: i64,
        generation: u64,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.reorder_pending = false;
        self.drop_target = None;
        self.card_drag_direction = 0;
        match result {
            Ok(()) if generation == self.history.generation => {
                self.feedback_revision += 1;
                self.reorder_offsets.clear();
                for (index, item) in self.history.items.loaded() {
                    if let Some(old_top) = self.reorder_before.get(&item.id) {
                        let offset = old_top - f32::from(self.row_offsets[index]);
                        if offset != 0. {
                            self.reorder_offsets.insert(item.id, offset);
                        }
                    }
                }
                if self.history.item_index(from).is_some() {
                    self.history.selected = Some(from);
                }
                self.feedback_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(visual::DRAG_MOTION_DURATION)
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        this.reorder_offsets.clear();
                        cx.notify();
                    });
                }));
                self.is_error = false;
            }
            Ok(()) => {}
            Err(error) => {
                self.set_feedback_message(error);
                self.is_error = true;
            }
        }
        self.reorder_before.clear();
        true
    }
    pub(super) fn valid_drag(&self, drag: &HistoryDrag) -> bool {
        !self.todos_open
            && !self.reorder_pending
            && !self.history.loading
            && drag.generation == self.history.generation
            && drag.favorite_only == self.history.favorite_only()
            && drag.group_id == self.history.group_id()
            && self
                .history
                .items
                .get(drag.card.card_index)
                .and_then(Option::as_ref)
                .is_some_and(|item| {
                    item.id == drag.id && item.pinned_in(drag.favorite_only) == drag.pinned
                })
    }

    pub(super) fn favorite_history_drag_active(&self, cx: &App) -> bool {
        self.active_history_drag().is_some()
            || (self.history_drag_uses_gpui && cx.has_active_drag())
    }

    pub(super) fn history_favorite_drop_at(
        &self,
        drag: &HistoryDrag,
        pointer: Point<Pixels>,
    ) -> bool {
        self.favorite_drop.contains(pointer)
            && !self.batch_mode
            && self.group_move_id.is_none()
            && self.valid_drag(drag)
    }

    pub(super) fn set_favorite_drop_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.favorite_drop.hovered.replace(hovered) != hovered {
            self.list_chrome.update(cx, |_, cx| cx.notify());
            if self.external_drag.is_some()
                && let Some(popup) = self.drag_preview_window
            {
                let _ = popup.update(cx, |_, _, cx| cx.notify());
            }
        }
    }

    pub(super) fn drop_history_favorite_at(
        &mut self,
        drag: &HistoryDrag,
        pointer: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.history_favorite_drop_at(drag, pointer)
            || self.locate_progress().is_some_and(|(_, total)| total > 1)
        {
            return false;
        }
        self.favorite_drop.hovered.set(false);
        self.send(
            Command::SetFavorite {
                id: drag.id,
                favorite: !drag.favorite_only,
            },
            cx,
        );
        cx.notify();
        true
    }

    pub(super) fn history_drag_target(
        &mut self,
        drag: &HistoryDrag,
        viewport_top: Pixels,
        pointer_y: Pixels,
    ) -> Option<DropTarget> {
        if !self.valid_drag(drag) {
            return None;
        }
        let source = drag.card.card_index;
        let content_y = pointer_y - viewport_top - self.scroll.offset().y;
        let items = &self.history.items;
        self.drag_motion.hover_target(
            source,
            |index| {
                items
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(|item| (item.id, item.pinned_in(drag.favorite_only)))
            },
            &self.row_offsets,
            content_y,
            self.drop_target,
        )
    }

    pub(super) fn drop_history_card(&mut self, drag: &HistoryDrag, cx: &mut Context<Self>) {
        if self.native_drag_active() {
            return;
        }
        self.favorite_drop.hovered.set(false);
        self.card_drag_direction = 0;
        let target = self.drop_target.take();
        if !self.valid_drag(drag) {
            self.set_feedback_message(
                tr(
                    self.language,
                    "列表已变化，请重新拖动",
                    "The list changed; drag again.",
                )
                .into(),
            );
            self.is_error = true;
        } else if let Some(target) = target.filter(|target| {
            target.id != drag.id
                && self.history.items.loaded().any(|(_, item)| {
                    item.id == target.id && item.pinned_in(drag.favorite_only) == drag.pinned
                })
        }) && self.send(
            Command::Reorder {
                from: drag.id,
                to: target.id,
                after: target.after,
                favorite_only: drag.favorite_only,
                group_id: drag.group_id,
                generation: drag.generation,
            },
            cx,
        ) {
            self.drag_motion.update(
                Some((drag.id, target)),
                |id| self.history.item_index(id),
                &self.row_offsets,
            );
            self.reorder_pending = true;
            self.drop_target = Some(target);
            let placement = self.drag_motion.to;
            self.reorder_before.clear();
            self.reorder_before
                .extend(self.history.items.loaded().map(|(index, item)| {
                    let shift = placement.map_or(0., |p| p.shift(index, &self.row_offsets));
                    (item.id, f32::from(self.row_offsets[index]) + shift)
                }));
        }
        cx.notify();
    }

    pub(super) fn start_card_drag_scroll(&mut self, cx: &mut Context<Self>) {
        self.card_drag_direction = 0;
        self.card_drag_scroll_task = Some(cx.spawn(async move |view, cx| {
            let mut ticks = 0usize;
            let mut scroll_target = None;
            loop {
                cx.background_executor()
                    .timer(visual::DRAG_SCROLL_INTERVAL)
                    .await;
                let Ok(active) = view.update(cx, |this, cx| {
                    if !this.card_drag_active(cx) {
                        if std::mem::take(&mut this.history_drag_uses_gpui) {
                            this.favorite_drop.hovered.set(false);
                            cx.notify();
                        }
                        this.card_drag_direction = 0;
                        if !this.reorder_pending {
                            this.drop_target = None;
                        }
                        this.list_surface.update(cx, |_, cx| cx.notify());
                        return false;
                    }
                    if this.native_drag_active() || this.card_drag_direction == 0 {
                        ticks = 0;
                        scroll_target = None;
                        return true;
                    }
                    ticks += 1;
                    if !ticks.is_multiple_of(visual::HISTORY_DRAG_SCROLL_TICKS) {
                        return true;
                    }
                    if this.todos_open {
                        this.scroll_todo_drag(cx);
                        return true;
                    }
                    let current = scroll_target.unwrap_or_else(|| {
                        history_top_row(&this.row_offsets, this.scroll.offset().y).0
                    });
                    let next = next_drag_scroll_index(
                        current,
                        this.history.items.len(),
                        this.card_drag_direction,
                    );
                    if next != current {
                        scroll_target = Some(next);
                        this.scroll_to_row(next);
                    }
                    let target = drag_edge_target_index(
                        next,
                        this.history.items.len(),
                        this.page_step().max(1) as usize,
                        this.card_drag_direction,
                    )
                    .and_then(|index| this.history.items.get(index))
                    .and_then(Option::as_ref)
                    .and_then(|target| {
                        let source = if let Some(drag) = this.active_history_drag() {
                            if !this.valid_drag(drag) {
                                return None;
                            }
                            this.history.items.get(drag.card.card_index)?.as_ref()?
                        } else {
                            this.drag_source_id().and_then(|id| {
                                this.history
                                    .items
                                    .loaded()
                                    .find_map(|(_, item)| (item.id == id).then_some(item))
                            })?
                        };
                        let favorites = this.history.favorite_only();
                        (source.id != target.id
                            && source.pinned_in(favorites) == target.pinned_in(favorites))
                        .then_some(DropTarget {
                            id: target.id,
                            after: this.card_drag_direction > 0,
                        })
                    })
                    .or(this.drop_target);
                    if next != current || this.drop_target != target {
                        this.drop_target = target;
                        this.list_surface.update(cx, |_, cx| cx.notify());
                    }
                    true
                }) else {
                    break;
                };
                if !active {
                    break;
                }
            }
        }));
    }
}
