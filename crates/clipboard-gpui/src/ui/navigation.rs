use super::*;
pub(super) fn history_row_height(
    item: &clipboard_core::database::ClipboardItem,
    display: DisplayPreference,
) -> f32 {
    if item.content_type == "image" {
        visual::image_row_height(display.card_density)
    } else {
        visual::row_height(display.card_density, display.card_max_lines)
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
    for item in items {
        let height = px(history_row_height(item, display));
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

impl ClipboardView {
    pub(super) fn query(&mut self, cx: &mut Context<Self>) {
        let generation = self.history.generation;
        if !self.send(
            Command::Query {
                search: self.search.read(cx).value().to_string(),
                limit: self.history.limit,
                favorite_only: self.history.favorite_only(),
                category: self.history.category(),
                group_id: self.history.group_id(),
                generation,
            },
            cx,
        ) {
            self.history.fail_query(generation);
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
        if selected {
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.close_history_confirmation(window, cx);
        self.group_move_id = None;
        self.reset_selection();
        self.search_task = None;
        if let Some(category) = category {
            self.history.set_category(category);
        } else {
            self.history.set_favorite_filter(true);
        }
        self.refresh_group_select(window, cx);
        self.scroll_to_top();
        self.query(cx);
        window.focus(&self.list_focus, cx);
        cx.notify();
    }

    pub(super) fn category_tab_index(&self) -> usize {
        if self.history.favorite_only() {
            1
        } else {
            match self.history.category() {
                ContentCategory::All => 0,
                ContentCategory::Text => 2,
                ContentCategory::Other => 3,
            }
        }
    }

    pub(super) fn select_adjacent_category(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.display.show_category_filter {
            return;
        }
        if let Some(next) = self.category_tab_index().checked_add_signed(direction)
            && let Some(&category) = CATEGORY_FILTERS.get(next)
        {
            self.select_category(category, window, cx);
        }
    }
    pub(super) fn scroll_to_top(&self) {
        self.scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(super) fn scroll_to_row(&self, index: usize) {
        if let Some(offset) = self.row_offsets.get(index) {
            self.scroll.set_offset(point(px(0.), -*offset));
        }
    }

    pub(super) fn rebuild_row_layout(&mut self) {
        (self.row_sizes, self.row_offsets) = history_row_layout(&self.history.items, self.display);
        self.drag_motion = DragMotionState::default();
    }

    pub(super) fn select(
        &mut self,
        direction: isize,
        strategy: ScrollStrategy,
        cx: &mut Context<Self>,
    ) {
        self.cancel_pending_row_click();
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
        self.cancel_pending_row_click();
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
    use super::{DragPlacement, history_top_row, px};

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
