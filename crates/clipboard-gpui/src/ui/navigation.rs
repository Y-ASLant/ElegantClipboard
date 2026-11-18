pub(super) const HISTORY_REPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

use super::category_tabs::ToolbarTab;
use super::*;
pub(super) fn history_row_height(
    item: &clipboard_core::database::ClipboardItem,
    display: DisplayPreference,
) -> f32 {
    let preview = item.preview.as_deref().unwrap_or("");
    if item.content_type == "image" {
        visual::media_row_height(display.card_density, true, 1)
    } else if item.content_type == "files" {
        let lines = visual::capped_preview_lines(
            preview,
            display.card_max_lines,
            visual::FILE_COLUMNS_PER_LINE,
        );
        visual::media_row_height(display.card_density, false, lines)
    } else {
        let lines = visual::capped_preview_lines(
            preview,
            display.card_max_lines,
            visual::TEXT_COLUMNS_PER_LINE,
        );
        visual::row_height(display.card_density, lines)
    }
}

pub(super) fn history_row_layout(
    items: &[clipboard_core::database::ClipboardItem],
    display: DisplayPreference,
) -> (Rc<Vec<gpui_kit::Size<Pixels>>>, Vec<Pixels>) {
    let mut sizes = Vec::with_capacity(items.len());
    let mut offsets = Vec::with_capacity(items.len() + 1);
    let mut top = px(0.);
    offsets.push(top);
    let trailing = px(visual::last_row_extra(display.card_density));
    for (index, item) in items.iter().enumerate() {
        let mut height = px(history_row_height(item, display));
        if index + 1 == items.len() {
            height += trailing;
        }
        sizes.push(size(px(0.), height));
        top += height;
        offsets.push(top);
    }
    (Rc::new(sizes), offsets)
}

pub(super) fn history_top_row(offsets: &[Pixels], scroll_y: Pixels) -> (usize, Pixels) {
    let top = -scroll_y;
    let index = offsets
        .partition_point(|offset| *offset <= top)
        .saturating_sub(1);
    (
        index.min(offsets.len().saturating_sub(2)),
        top - offsets[index],
    )
}

/// One mouse-wheel notch moves farther than a few dozen pixels, so a small
/// fixed threshold covers the last row's source badge on the first tick.
/// Show the control only after the first screen has left and another screen
/// remains below.
pub(super) fn show_history_scroll_to_top(
    offset_y: Pixels,
    max_offset_y: Pixels,
    viewport_height: Pixels,
) -> bool {
    let scrolled = -offset_y;
    viewport_height > px(0.) && max_offset_y > viewport_height && scrolled > viewport_height
}

/// Scroll distance that centres row `index` in the viewport without leaving the list.
pub(super) fn centered_row_scroll(
    offsets: &[Pixels],
    index: usize,
    viewport_height: Pixels,
) -> Option<Pixels> {
    let top = *offsets.get(index)?;
    let bottom = *offsets.get(index + 1)?;
    let slack = (viewport_height - (bottom - top)).max(px(0.));
    let furthest = (*offsets.last()? - viewport_height).max(px(0.));
    Some((top - slack * 0.5).max(px(0.)).min(furthest))
}
impl ClipboardView {
    pub(super) fn query(&mut self, cx: &mut Context<Self>) -> bool {
        let generation = self.history.generation;
        if !self.send(
            Command::Query {
                // A located row shows the full list; the typed search is kept for later.
                search: if self.located.is_some() {
                    String::new()
                } else {
                    self.search.read(cx).value().to_string()
                },
                limit: 0,
                favorite_only: self.history.favorite_only(),
                category: self.history.category(),
                group_id: self.history.group_id(),
                generation,
            },
            cx,
        ) {
            self.history.fail_query(generation);
            self.history_reply_missed = true;
            return false;
        }
        self.history_reply_missed = false;
        true
    }

    pub(super) fn history_needs_reload(&self) -> bool {
        self.history.loading || self.history_reply_missed
    }

    /// Send the query for `reload`, then end loading if that generation never replies.
    /// Search passes `debounce` so a burst of keystrokes shares one request.
    pub(super) fn arm_history_query(
        &mut self,
        reload: HistoryReload,
        debounce: bool,
        cx: &mut Context<Self>,
    ) {
        let generation = reload.generation();
        if !debounce {
            if self.query(cx) {
                self.arm_history_reply_timeout(generation, cx);
            }
            return;
        }
        self.history_load_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let sent = view
                .update(cx, |this, cx| {
                    if this.history.generation != generation || !this.history.loading {
                        return false;
                    }
                    this.query(cx)
                })
                .unwrap_or(false);
            if !sent {
                return;
            }
            cx.background_executor().timer(HISTORY_REPLY_TIMEOUT).await;
            let _ = view.update(cx, |this, cx| this.fail_unanswered_history(generation, cx));
        }));
    }

    pub(super) fn reload_current_history(&mut self, cx: &mut Context<Self>) {
        let generation = self.history.generation;
        if self.query(cx) {
            self.arm_history_reply_timeout(generation, cx);
        }
    }

    fn arm_history_reply_timeout(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.history_load_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(HISTORY_REPLY_TIMEOUT).await;
            let _ = view.update(cx, |this, cx| this.fail_unanswered_history(generation, cx));
        }));
    }

    pub(super) fn fail_unanswered_history(&mut self, generation: u64, cx: &mut Context<Self>) {
        if self.history.generation != generation || !self.history.loading {
            return;
        }
        self.history.fail_query(generation);
        self.history_reply_missed = true;
        self.set_feedback_message(
            tr(
                self.language,
                "历史列表没有返回",
                "The history list did not load",
            )
            .into(),
        );
        self.is_error = true;
        cx.notify();
    }

    /// Drop a drag session before the history generation changes, so a parked
    /// snapshot cannot land on the next scope.
    pub(super) fn release_history_drag(&mut self, cx: &mut Context<Self>) {
        if self.external_drag.is_some() {
            self.finish_external_drag(cx);
        }
    }

    pub(super) fn select_category(
        &mut self,
        category: Option<ContentCategory>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_delete_pending || self.clear_pending || self.group_reorder_pending {
            return;
        }
        let selected = match category {
            None => self.history.favorite_only(),
            Some(category) => !self.history.favorite_only() && self.history.category() == category,
        };
        if selected && !self.todos_open {
            if self.history_needs_reload() {
                self.scroll_to_top();
                self.reload_current_history(cx);
                window.focus(&self.list_focus, cx);
                cx.notify();
            }
            return;
        }
        if !selected || self.todos_open {
            self.release_history_drag(cx);
            self.drag_motion = DragMotionState::default();
        }
        self.cancel_sequence(cx);
        self.close_hover_preview("category_changed", cx);
        self.close_history_confirmation(window, cx);
        self.group_move_id = None;
        self.located = None;
        self.todos_open = false;
        self.selected_todo = None;
        self.sync_search_prompt(window, cx);
        if selected {
            self.scroll_to_top();
            if self.history_needs_reload() {
                self.reload_current_history(cx);
            }
            window.focus(&self.list_focus, cx);
            cx.notify();
            return;
        }
        let reload = if let Some(category) = category {
            self.history.set_category(category)
        } else {
            self.history.set_favorite_filter(true)
        };
        self.scroll_to_top();
        if let Some(reload) = reload {
            self.arm_history_query(reload, false, cx);
        } else if self.history_needs_reload() {
            self.reload_current_history(cx);
        }
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    /// Leave the search and category filter for the full list of the current
    /// group, then land on `id` once that list arrives so its neighbours show.
    pub(super) fn locate_in_timeline(
        &mut self,
        id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_history_drag(cx);
        self.cancel_sequence(cx);
        self.close_hover_preview("locate_in_timeline", cx);
        self.close_history_confirmation(window, cx);
        self.group_move_id = None;
        self.located = Some(id);
        // Captured before the reload below empties the filtered list.
        self.located_matches = self.history.items.iter().map(|item| item.id).collect();
        let reload = match self.history.set_category(ContentCategory::All) {
            Some(reload) => reload,
            None => self.history.begin_search(),
        };
        self.scroll_to_top();
        self.locate_id = Some(id);
        self.arm_history_query(reload, false, cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    /// Select the row a pending `locate_in_timeline` asked for and centre it.
    pub(super) fn finish_locate(&mut self) {
        let Some(id) = self.locate_id.take() else {
            return;
        };
        let Some(index) = self.history.items.iter().position(|item| item.id == id) else {
            return;
        };
        self.history.selected = Some(id);
        self.center_row(index);
    }

    fn center_row(&mut self, index: usize) {
        self.scroll_to_top_task = None;
        if let Some(top) =
            centered_row_scroll(&self.row_offsets, index, self.scroll.bounds().size.height)
        {
            self.scroll.set_offset(point(px(0.), -top));
        }
    }

    /// Rows of the full list that were in the filtered result, in list order.
    fn located_rows(&self) -> Vec<(usize, i64)> {
        self.history
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.located_matches.contains(&item.id))
            .map(|(index, item)| (index, item.id))
            .collect()
    }

    /// One-based position of the located row among those matches, and their count.
    pub(super) fn locate_progress(&self) -> Option<(usize, usize)> {
        let current = self.located?;
        let rows = self.located_rows();
        let position = rows.iter().position(|(_, id)| *id == current)?;
        Some((position + 1, rows.len()))
    }

    /// Move the located mark to the previous or next match in the timeline.
    pub(super) fn locate_step(&mut self, direction: isize, cx: &mut Context<Self>) {
        let Some(current) = self.located else {
            return;
        };
        let rows = self.located_rows();
        let Some(&(index, id)) = rows
            .iter()
            .position(|(_, id)| *id == current)
            .and_then(|position| position.checked_add_signed(direction))
            .and_then(|position| rows.get(position))
        else {
            return;
        };
        self.close_hover_preview("locate_step", cx);
        self.located = Some(id);
        self.history.selected = Some(id);
        self.center_row(index);
        cx.notify();
    }

    pub(super) fn select_adjacent_category(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.display.value.show_category_filter {
            return;
        }
        let tabs = self.toolbar_tabs();
        if let Some(next) = self.toolbar_tab_index().checked_add_signed(direction) {
            match tabs.get(next) {
                Some(ToolbarTab::Todos) => self.open_todos(window, cx),
                Some(ToolbarTab::Category(filter)) => self.select_category(*filter, window, cx),
                None => {}
            }
        }
    }
    pub(super) fn scroll_to_top(&mut self) {
        self.scroll_to_top_task = None;
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(super) fn animate_scroll_to_top(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_top_task = None;
        self.close_hover_preview("scroll_to_top", cx);
        let start = self.scroll.offset();
        if cx.reduce_motion() || start.y >= px(0.) {
            self.scroll_to_top();
            cx.notify();
            return;
        }
        self.scroll_to_top_task = Some(cx.spawn(async move |view, cx| {
            let started = std::time::Instant::now();
            let mut expected = start;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let progress = (started.elapsed().as_secs_f32()
                    / crate::design_system::SLOW.as_secs_f32())
                .min(1.);
                let remaining = (1. - progress).powi(3);
                let next = point(start.x, start.y * remaining);
                let keep = view
                    .update(cx, |this, cx| {
                        // Scrollbar drags, keyboard navigation and other programmatic
                        // scrolls take precedence over an in-flight return animation.
                        if this.scroll.offset() != expected {
                            return false;
                        }
                        this.scroll.set_offset(next);
                        cx.notify();
                        progress < 1.
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
                expected = next;
            }
        }));
    }

    pub(super) fn scroll_to_row(&mut self, index: usize) {
        self.scroll_to_top_task = None;
        if let Some(offset) = self.row_offsets.get(index) {
            self.scroll.set_offset(point(px(0.), -*offset));
        }
    }

    pub(super) fn rebuild_row_layout(&mut self) {
        (self.row_sizes, self.row_offsets) =
            history_row_layout(&self.history.items, self.display.value);
        self.drag_motion = DragMotionState::default();
    }

    pub(super) fn select(
        &mut self,
        direction: isize,
        strategy: ScrollStrategy,
        cx: &mut Context<Self>,
    ) {
        self.scroll_to_top_task = None;
        if let Some(index) = self.history.select_relative(direction) {
            self.scroll.scroll_to_item(index, strategy);
        }
        cx.notify();
    }

    pub(super) fn select_previous_or_focus_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.items.first().map(|item| item.id) == self.history.selected
            && self.history.selected.is_some()
        {
            self.search
                .update(cx, |search, cx| search.focus(window, cx));
        } else {
            self.select(-1, ScrollStrategy::Nearest, cx);
        }
    }

    pub(super) fn focus_first_history_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.todos_open {
            let Some(&id) = self.todo_layout.borrow().ids.first() else {
                return;
            };
            self.selected_todo = Some(id);
            self.todo_scroll.scroll_to_item(0, ScrollStrategy::Top);
            window.focus(&self.list_focus, cx);
            cx.notify();
            return;
        }
        if self.history.loading || self.history.items.is_empty() {
            return;
        }
        self.select_index(0, ScrollStrategy::Top, cx);
        window.focus(&self.list_focus, cx);
    }

    pub(super) fn select_index(
        &mut self,
        index: usize,
        strategy: ScrollStrategy,
        cx: &mut Context<Self>,
    ) {
        self.scroll_to_top_task = None;
        if let Some(index) = self.history.select_index(index) {
            if strategy == ScrollStrategy::Top {
                self.scroll_to_row(index);
            } else {
                self.scroll.scroll_to_item(index, strategy);
            }
        }
        cx.notify();
    }

    pub(super) fn page_step(&self) -> isize {
        let (top, _) = history_top_row(&self.row_offsets, self.scroll.offset().y);
        let viewport_end = -self.scroll.offset().y + self.scroll.bounds().size.height;
        let visible = self
            .row_offsets
            .partition_point(|offset| *offset < viewport_end);
        visible.saturating_sub(top + 2).max(1) as isize
    }
}
#[cfg(test)]
mod history_virtual_list_tests {
    use super::{
        DragPlacement, centered_row_scroll, history_top_row, px, show_history_scroll_to_top,
    };

    #[test]
    fn located_row_is_centred_but_never_scrolls_past_either_end() {
        let rows = [px(0.), px(100.), px(200.), px(300.), px(400.), px(500.)];
        let viewport = px(200.);
        assert_eq!(centered_row_scroll(&rows, 0, viewport), Some(px(0.)));
        assert_eq!(centered_row_scroll(&rows, 2, viewport), Some(px(150.)));
        assert_eq!(centered_row_scroll(&rows, 4, viewport), Some(px(300.)));
        assert_eq!(centered_row_scroll(&rows, 5, viewport), None);
        // A row taller than the viewport starts at its own top.
        let tall = [px(0.), px(100.), px(500.)];
        assert_eq!(centered_row_scroll(&tall, 1, viewport), Some(px(100.)));
    }

    #[test]
    fn scroll_to_top_stays_hidden_for_a_wheel_notch_and_a_short_list() {
        let viewport = px(420.);
        assert!(!show_history_scroll_to_top(px(0.), px(80.), viewport));
        assert!(!show_history_scroll_to_top(px(-32.), px(2000.), viewport));
        assert!(!show_history_scroll_to_top(px(-420.), px(2000.), viewport));
        assert!(show_history_scroll_to_top(px(-421.), px(2000.), viewport));
        assert!(!show_history_scroll_to_top(px(-800.), px(400.), viewport));
        assert!(!show_history_scroll_to_top(px(-800.), px(2000.), px(0.)));
    }

    #[test]
    fn mixed_height_rows_preserve_partial_position_across_resizing_and_pagination() {
        let old = [px(0.), px(88.), px(236.), px(324.)];
        let (row, partial) = history_top_row(&old, px(-125.));
        assert_eq!((row, partial), (1, px(37.)));

        let resized = [px(0.), px(112.), px(260.), px(348.), px(460.)];
        let restored = -(resized[row] + partial);
        assert_eq!(history_top_row(&resized, restored), (1, px(37.)));
        assert_eq!(history_top_row(&resized, px(-348.)), (3, px(0.)));
    }

    #[test]
    fn dragging_text_past_an_image_yields_exactly_the_source_height() {
        let offsets = [px(0.), px(96.), px(220.), px(308.), px(456.)];
        let down = DragPlacement::new(0, 2, true).unwrap();
        assert_eq!(
            std::array::from_fn(|index| down.shift(index, &offsets)),
            [212., -96., -96., 0.]
        );
        let up = DragPlacement::new(3, 1, false).unwrap();
        assert_eq!(
            std::array::from_fn(|index| up.shift(index, &offsets)),
            [0., 148., 148., -212.]
        );
        assert!(DragPlacement::new(0, 1, false).is_none());
        assert!(DragPlacement::new(2, 1, true).is_none());
    }
}
