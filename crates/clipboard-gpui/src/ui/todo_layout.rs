use super::*;

#[derive(Default)]
pub(super) struct TodoListLayout {
    pub ids: Vec<i64>,
    pub sources: Vec<usize>,
    pub indexes: HashMap<i64, usize>,
    pub sizes: Rc<Vec<gpui_kit::Size<Pixels>>>,
    pub offsets: Vec<Pixels>,
    pub heights: HashMap<i64, f32>,
    pub geometry_dirty: bool,
    filter_dirty: bool,
    query: String,
    show_completed: bool,
    key: Option<LayoutKey>,
}

#[derive(PartialEq)]
struct LayoutKey {
    width: Pixels,
    rem: Pixels,
    density: CardDensity,
    max_lines: u8,
    show_created: bool,
    show_edited: bool,
    fonts: FontPreference,
}

impl TodoListLayout {
    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    pub fn has_current_filter(&self) -> bool {
        !self.filter_dirty
    }

    pub fn record_height(&mut self, id: i64, height: f32) -> bool {
        if height <= 0. || self.heights.get(&id).copied() == Some(height) {
            return false;
        }
        self.heights.insert(id, height);
        self.geometry_dirty = true;
        true
    }

    pub fn retain_measurements(&mut self, before: &[Todo], after: &[Todo]) {
        let previous: HashMap<_, _> = before.iter().map(|todo| (todo.id, todo)).collect();
        let current: HashMap<_, _> = after.iter().map(|todo| (todo.id, todo)).collect();
        self.heights.retain(|id, _| {
            current
                .get(id)
                .is_some_and(|todo| previous.get(id) == Some(todo))
        });
        self.filter_dirty = true;
    }
}

impl ClipboardView {
    /// Sole owner of filtered indexes and the geometry used by render and dragging.
    pub(super) fn sync_todo_layout(&mut self, window: &Window, cx: &App) {
        let mut layout = self.todo_layout.borrow_mut();
        let query = self.search.read(cx).value();
        let query = query.trim();
        let preference = self.todo_preference.value;
        let density = preference.density(self.display.value.card_density);
        let style_changed = layout.key.as_ref().is_none_or(|key| {
            key.width != window.viewport_size().width
                || key.rem != window.rem_size()
                || key.density != density
                || key.max_lines != self.display.value.card_max_lines
                || key.show_created != preference.show_created
                || key.show_edited != preference.show_edited
                || key.fonts != self.fonts.value
        });
        let projection_changed =
            layout.query != query || layout.show_completed != preference.show_completed;
        let filter_changed = layout.filter_dirty || projection_changed;
        if !filter_changed && !style_changed && !layout.geometry_dirty {
            return;
        }
        if style_changed {
            layout.heights.clear();
            layout.key = Some(LayoutKey {
                width: window.viewport_size().width,
                rem: window.rem_size(),
                density,
                max_lines: self.display.value.card_max_lines,
                show_created: preference.show_created,
                show_edited: preference.show_edited,
                fonts: self.fonts.value.clone(),
            });
        }
        if filter_changed {
            layout.show_completed = preference.show_completed;
            layout.query.clear();
            layout.query.push_str(query);
            let normalized = query.to_lowercase();
            layout.ids.clear();
            layout.sources.clear();
            layout.indexes.clear();
            for (source, todo) in self.todos.iter().enumerate() {
                if (self.todo_preference.value.show_completed || !todo.status.is_finished())
                    && (normalized.is_empty() || todo.text.to_lowercase().contains(&normalized))
                {
                    let index = layout.ids.len();
                    layout.ids.push(todo.id);
                    layout.sources.push(source);
                    layout.indexes.insert(todo.id, index);
                }
            }
        }
        let scroll_anchor = (!filter_changed && !layout.offsets.is_empty())
            .then(|| todo_scroll_anchor(&layout.offsets, self.todo_scroll.offset().y));
        let fallback = super::todo_motion::todo_row_fallback(self);
        let mut sizes = Vec::with_capacity(layout.ids.len() + 1);
        let mut offsets = Vec::with_capacity(layout.ids.len() + 1);
        let mut top = px(0.);
        offsets.push(top);
        for id in &layout.ids {
            let height = px(layout.heights.get(id).copied().unwrap_or(fallback));
            sizes.push(size(window.viewport_size().width, height));
            top += height;
            offsets.push(top);
        }
        // A non-card footer preserves clearance without extending a draggable row.
        sizes.push(size(
            window.viewport_size().width,
            px(visual::CONTROL_HEIGHT_LARGE + visual::SPACE_6),
        ));
        layout.sizes = Rc::new(sizes);
        layout.offsets = offsets;
        if let Some((index, partial)) = scroll_anchor {
            let top = layout.offsets[index.min(layout.ids.len())];
            self.todo_scroll.set_offset(point(px(0.), -(top + partial)));
        }
        layout.filter_dirty = false;
        layout.geometry_dirty = false;
        drop(layout);
        if filter_changed || style_changed {
            self.drag_motion = DragMotionState::default();
            self.todo_drop_target = None;
        }
        if projection_changed || style_changed {
            self.todo_move_offsets.clear();
        }
    }
}

fn todo_scroll_anchor(offsets: &[Pixels], scroll_y: Pixels) -> (usize, Pixels) {
    let top = -scroll_y;
    let index = offsets
        .partition_point(|offset| *offset <= top)
        .saturating_sub(1);
    (index, top - offsets[index])
}

#[cfg(test)]
mod tests {
    use super::{Todo, TodoListLayout, TodoStatus, px, todo_scroll_anchor};
    use std::collections::HashMap;
    #[test]
    fn scroll_anchor_preserves_partial_rows_and_footer_clearance() {
        let offsets = [px(0.), px(64.), px(172.)];
        assert_eq!(todo_scroll_anchor(&offsets, px(-93.)), (1, px(29.)));
        assert_eq!(todo_scroll_anchor(&offsets, px(-190.)), (2, px(18.)));
    }

    #[test]
    fn same_count_replacement_drops_removed_and_edited_measurements() {
        let todo = |id, text: &str| Todo {
            id,
            text: text.into(),
            status: TodoStatus::Pending,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let mut layout = TodoListLayout::default();
        layout.heights.extend([(1, 40.), (2, 60.), (3, 80.)]);
        layout.retain_measurements(
            &[todo(1, "same"), todo(2, "old"), todo(3, "removed")],
            &[todo(1, "same"), todo(2, "edited"), todo(4, "new")],
        );
        assert_eq!(layout.heights, HashMap::from([(1, 40.)]));
        assert!(layout.filter_dirty);
    }
}
