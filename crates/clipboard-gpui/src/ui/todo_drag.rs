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
    pub(super) fn valid_todo_drag(&self, drag: &TodoDrag, _cx: &App) -> bool {
        !self.todos_pending
            && !self.todo_reorder_pending
            && self.todos_open
            && self.todo_layout.borrow().has_current_filter()
            && self.todo_layout.borrow().indexes.contains_key(&drag.id)
    }

    pub(super) fn todo_drag_target(
        &mut self,
        drag: &TodoDrag,
        list_top: Pixels,
        pointer_y: Pixels,
        _cx: &App,
    ) -> Option<DropTarget> {
        let layout = self.todo_layout.borrow();
        if !layout.has_current_filter() {
            return None;
        }
        let source = *layout.indexes.get(&drag.id)?;
        let content_y = pointer_y - list_top - self.todo_scroll.offset().y;
        self.drag_motion.hover_target(
            source,
            |index| {
                let todo = self.todos.get(*layout.sources.get(index)?)?;
                Some((todo.id, todo.status))
            },
            &layout.offsets,
            content_y,
            self.todo_drop_target,
        )
    }

    pub(super) fn scroll_todo_drag(&mut self, cx: &mut Context<Self>) {
        let current = self.todo_scroll.offset();
        let layout = self.todo_layout.borrow();
        let height = self
            .todo_drag_source_id()
            .and_then(|id| layout.indexes.get(&id).copied())
            .map(|index| layout.offsets[index + 1] - layout.offsets[index])
            .unwrap_or_else(|| px(super::todo_motion::todo_row_fallback(self)));
        let step = height * f32::from(self.card_drag_direction);
        drop(layout);
        let y = (current.y - step).clamp(-self.todo_scroll.max_offset().y, px(0.));
        if y != current.y {
            self.todo_scroll.set_offset(point(current.x, y));
            self.list_surface.update(cx, |_, cx| cx.notify());
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
            let layout = self.todo_layout.borrow();
            self.drag_motion.update(
                Some((drag.id, target)),
                |id| layout.ids.iter().position(|&row| row == id),
                &layout.offsets,
            );
            drop(layout);
            self.todo_reorder_pending = true;
            self.todo_drop_target = Some(target);
        }
        cx.notify();
    }
}
