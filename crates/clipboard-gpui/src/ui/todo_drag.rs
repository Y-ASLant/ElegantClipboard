use super::todo_card::{TodoDragSnapshot, todo_row_surface};
use super::*;

#[derive(Clone)]
pub(super) struct TodoDrag {
    pub id: i64,
    pub snapshot: TodoDragSnapshot,
    pub size: Rc<Cell<gpui_kit::Size<Pixels>>>,
    pub native_preview: bool,
}

impl TodoDrag {
    pub(super) fn render_card(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let density = self
            .snapshot
            .todo_preference
            .density(self.snapshot.display.card_density);
        drag_visual::drag_card(self.size.get(), density, cx)
            .child(todo_row_surface(&self.snapshot, true, window, cx))
            .into_any_element()
    }
}

impl Render for TodoDrag {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.native_preview {
            div().into_any_element()
        } else {
            self.render_card(window, cx)
        }
    }
}

impl ClipboardView {
    pub(super) fn valid_todo_drag(&self, drag: &TodoDrag, cx: &App) -> bool {
        !self.todos_pending
            && !self.todo_reorder_pending
            && self.todos_open
            && visible_todos(self, cx).any(|todo| todo.id == drag.id)
    }

    pub(super) fn todo_row_offsets(&self, cx: &App) -> Vec<Pixels> {
        let heights = self.todo_heights.borrow();
        let fallback = super::todo_motion::todo_row_fallback(self);
        let mut offsets = vec![px(0.)];
        for todo in visible_todos(self, cx) {
            let height = heights
                .get(&todo.id)
                .copied()
                .filter(|height| *height > 0.)
                .unwrap_or(fallback);
            offsets.push(*offsets.last().expect("initial offset") + px(height));
        }
        offsets
    }

    pub(super) fn todo_drag_target(
        &mut self,
        drag: &TodoDrag,
        list_top: Pixels,
        pointer_y: Pixels,
        cx: &App,
    ) -> Option<DropTarget> {
        let offsets = self.todo_row_offsets(cx);
        let slots: Vec<_> = visible_todos(self, cx)
            .map(|todo| (todo.id, todo.completed))
            .collect();
        let source = slots.iter().position(|(id, _)| *id == drag.id)?;
        let content_y = pointer_y - list_top - self.todo_scroll.offset().y;
        self.drag_motion.hover_target(
            source,
            slots.iter().copied(),
            &offsets,
            content_y,
            self.todo_drop_target,
        )
    }

    pub(super) fn scroll_todo_drag(&mut self, cx: &mut Context<Self>) {
        let current = self.todo_scroll.offset();
        let step =
            px(super::todo_motion::todo_row_fallback(self) * f32::from(self.card_drag_direction));
        let y = (current.y - step).clamp(-self.todo_scroll.max_offset().y, px(0.));
        if y != current.y {
            self.todo_scroll.set_offset(point(current.x, y));
            cx.notify();
        }
    }

    pub(super) fn drop_todo(&mut self, drag: &TodoDrag, cx: &mut Context<Self>) {
        let Some(target) = self
            .todo_drop_target
            .take()
            .filter(|target| target.id != drag.id)
        else {
            cx.notify();
            return;
        };
        if !self.valid_todo_drag(drag, cx) {
            self.set_feedback_message(
                tr(
                    self.language,
                    "待办列表已变化，请重新拖动",
                    "The to-do list changed; drag again.",
                )
                .into(),
            );
            self.is_error = true;
            cx.notify();
            return;
        }
        if self.send(
            Command::ReorderTodo {
                from: drag.id,
                to: target.id,
                after: target.after,
            },
            cx,
        ) {
            let ids: Vec<_> = visible_todos(self, cx).map(|todo| todo.id).collect();
            self.drag_motion.update(
                Some((drag.id, target)),
                ids.iter().copied(),
                &self.todo_row_offsets(cx),
            );
            self.todo_reorder_pending = true;
            self.todo_drop_target = Some(target);
        }
        cx.notify();
    }
}

pub(super) fn visible_todos<'a>(
    view: &'a ClipboardView,
    cx: &App,
) -> impl Iterator<Item = &'a Todo> {
    let query = view.search.read(cx).value().trim().to_lowercase();
    let show_completed = view.todo_preference.value.show_completed;
    view.todos.iter().filter(move |todo| {
        (show_completed || !todo.completed)
            && (query.is_empty() || todo.text.to_lowercase().contains(&query))
    })
}
