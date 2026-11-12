use super::*;
use std::collections::HashMap;

impl ClipboardView {
    /// Slide cards whose visible order changed. Added or removed cards do not travel.
    pub(super) fn start_todo_slide(&mut self, next: &[Todo], cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            self.todo_move_offsets.clear();
            return;
        }
        let query = self.search.read(cx).value().trim().to_lowercase();
        let show_completed = self.todo_preference.value.show_completed;
        let mut before = visible_todo_ids(&self.todos, show_completed, &query);
        if self.todo_reorder_pending
            && let Some(placement) = self.drag_motion.to
            && placement.source < before.len()
            && placement.insertion < before.len()
        {
            let moved = before.remove(placement.source);
            before.insert(placement.insertion, moved);
        }
        let after = visible_todo_ids(next, show_completed, &query);
        let heights = self.todo_heights.borrow();
        let fallback = todo_row_fallback(self);
        let offsets = slide_offsets(&before, &after, |id| {
            heights
                .get(&id)
                .copied()
                .filter(|height| *height > 0.)
                .unwrap_or(fallback)
        });
        drop(heights);
        match offsets {
            Some(offsets) if !offsets.is_empty() => {
                self.todo_motion_revision = self.todo_motion_revision.wrapping_add(1);
                let revision = self.todo_motion_revision;
                self.todo_move_offsets = offsets;
                self.todo_motion_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(visual::DRAG_MOTION_DURATION)
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        if this.todo_motion_revision == revision {
                            this.todo_move_offsets.clear();
                            cx.notify();
                        }
                    });
                }));
            }
            None => self.todo_move_offsets.clear(),
            Some(_) => {}
        }
    }
}

fn visible_todo_ids(todos: &[Todo], show_completed: bool, query: &str) -> Vec<i64> {
    todos
        .iter()
        .filter(|todo| {
            (show_completed || !todo.completed)
                && (query.is_empty() || todo.text.to_lowercase().contains(query))
        })
        .map(|todo| todo.id)
        .collect()
}

pub(super) fn todo_row_fallback(view: &ClipboardView) -> f32 {
    let density = view
        .todo_preference
        .value
        .density(view.display.value.card_density);
    let preference = view.todo_preference.value;
    let body = visual::PREVIEW_LINE_HEIGHT
        + if preference.show_created || preference.show_edited {
            visual::card_spacing(density) + visual::METADATA_ROW_HEIGHT
        } else {
            0.
        };
    let spacing = visual::card_spacing(density);
    spacing * 2. + visual::card_padding(density) * 2. + visual::BORDER_WIDTH * 2. + body
}

/// `None` when the visible set changed. An empty map means the same order.
fn slide_offsets(
    before: &[i64],
    after: &[i64],
    stride: impl Fn(i64) -> f32,
) -> Option<HashMap<i64, f32>> {
    if before.len() != after.len() {
        return None;
    }
    let mut seen = HashMap::with_capacity(after.len());
    for id in after {
        if seen.insert(*id, ()).is_some() {
            return None;
        }
    }
    if before.iter().any(|id| !seen.contains_key(id)) {
        return None;
    }
    let rows: Vec<_> = after.iter().copied().map(|id| (id, stride(id))).collect();
    Some(crate::state::reorder_pixel_offsets(before, &rows))
}

#[cfg(test)]
mod tests {
    use super::slide_offsets;

    #[test]
    fn checking_the_first_card_slides_it_down_by_the_rows_it_passes() {
        let offsets = slide_offsets(&[1, 2], &[2, 1], |_| 48.).unwrap();
        assert_eq!(offsets.get(&1), Some(&-48.));
        assert_eq!(offsets.get(&2), Some(&48.));
        assert!(slide_offsets(&[1, 2], &[1, 2], |_| 48.).unwrap().is_empty());
        assert!(slide_offsets(&[1, 2], &[2], |_| 48.).is_none());
    }
}
