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
        self.history_drag_direction = 0;
        let before = self.reorder_before.take();
        match result {
            Ok(()) if generation == self.history.generation => {
                self.feedback_revision += 1;
                if let Some(before) = before {
                    let after: Vec<_> = self
                        .history
                        .items
                        .iter()
                        .map(|item| (item.id, history_row_height(item, self.display)))
                        .collect();
                    self.reorder_offsets = reorder_pixel_offsets(&before, &after);
                }
                if self.history.items.iter().any(|item| item.id == from) {
                    self.history.selected = Some(from);
                }
                self.feedback_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(visual::MOTION_DURATION)
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        this.reorder_offsets.clear();
                        cx.notify();
                    });
                }));
                self.message = tr(self.language, "顺序已保存", "Order saved").into();
                self.is_error = false;
            }
            Ok(()) => {}
            Err(error) => {
                self.message = error;
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn valid_drag(&self, drag: &HistoryDrag) -> bool {
        !self.reorder_pending
            && !self.history.loading
            && drag.generation == self.history.generation
            && drag.favorite_only == self.history.favorite_only()
            && drag.group_id == self.history.group_id()
            && self
                .history
                .items
                .iter()
                .any(|item| item.id == drag.id && item.is_pinned == drag.pinned)
    }

    pub(super) fn drop_history_card(&mut self, drag: &HistoryDrag, cx: &mut Context<Self>) {
        self.history_drag_direction = 0;
        let target = self.drop_target.take();
        if !self.valid_drag(drag) {
            self.message = tr(
                self.language,
                "列表已变化，请重新拖动",
                "The list changed; drag again.",
            )
            .into();
            self.is_error = true;
        } else if let Some(target) = target.filter(|target| target.allowed && target.id != drag.id)
            && self.send(
                Command::Reorder {
                    from: drag.id,
                    to: target.id,
                    after: target.after,
                    favorite_only: drag.favorite_only,
                    group_id: drag.group_id,
                    generation: drag.generation,
                },
                cx,
            )
        {
            self.reorder_pending = true;
            self.reorder_before = Some(self.history.items.iter().map(|item| item.id).collect());
        }
        cx.notify();
    }

    pub(super) fn start_history_drag_scroll(&mut self, cx: &mut Context<Self>) {
        self.history_drag_direction = 0;
        self.history_drag_scroll_task = Some(cx.spawn(async move |view, cx| {
            let mut ticks = 0usize;
            let mut scroll_target = None;
            loop {
                cx.background_executor()
                    .timer(visual::DRAG_SCROLL_INTERVAL)
                    .await;
                let Ok(active) = view.update(cx, |this, cx| {
                    if !cx.has_active_drag() {
                        this.history_drag_direction = 0;
                        this.drop_target = None;
                        cx.notify();
                        return false;
                    }
                    if this.history_drag_direction == 0 {
                        ticks = 0;
                        scroll_target = None;
                        return true;
                    }
                    ticks += 1;
                    if !ticks.is_multiple_of(visual::HISTORY_DRAG_SCROLL_TICKS) {
                        return true;
                    }
                    let current = scroll_target.unwrap_or_else(|| {
                        history_top_row(&this.row_offsets, this.scroll.offset().y).0
                    });
                    let next = next_drag_scroll_index(
                        current,
                        this.history.items.len(),
                        this.history_drag_direction,
                    );
                    if next != current {
                        scroll_target = Some(next);
                        this.scroll_to_row(next);
                    }
                    let target = drag_edge_target_index(
                        next,
                        this.history.items.len(),
                        this.page_step().max(1) as usize,
                        this.history_drag_direction,
                    )
                    .and_then(|index| this.history.items.get(index))
                    .and_then(|target| {
                        let source = this
                            .history
                            .selected
                            .and_then(|id| this.history.items.iter().find(|item| item.id == id))?;
                        (source.id != target.id).then_some(DropTarget {
                            id: target.id,
                            after: this.history_drag_direction > 0,
                            allowed: source.is_pinned == target.is_pinned,
                        })
                    });
                    if next != current || this.drop_target != target {
                        this.drop_target = target;
                        cx.notify();
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
