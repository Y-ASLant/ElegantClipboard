use super::*;
use gpui_kit::component::status_bar::StatusBar;

impl ClipboardView {
    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        if self.message.is_empty() && !self.reorder_pending {
            return div().into_any_element();
        }
        StatusBar::new()
            .flex_shrink_0()
            .px(px(PAGE_PADDING))
            .left(
                div()
                    .text_color(if self.is_error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(if self.reorder_pending {
                        tr(self.language, "正在保存顺序…", "Saving order…").to_owned()
                    } else {
                        self.message.clone()
                    }),
            )
            .into_any_element()
    }
}
impl Render for ClipboardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .confirmation
            .is_some_and(|kind| !self.confirmation_active(kind) && !self.confirmation_pending(kind))
        {
            cx.defer_in(window, |view, window, cx| {
                view.sync_confirmation_dialog(window, cx);
            });
        }
        let drag_request = if cx.has_active_drag() {
            self.history
                .selected
                .zip(self.drop_target.filter(|target| target.allowed))
        } else {
            None
        };
        self.drag_motion.update(drag_request, &self.history.items);
        window_shell(cx)
            .font_family("Microsoft YaHei UI")
            .child(
                div()
                    .id("window-drag-handle")
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_grab()
                    .window_control_area(WindowControlArea::Drag)
                    .child(
                        div()
                            .w(px(36.))
                            .h(px(4.))
                            .rounded_full()
                            .bg(cx.theme().border),
                    ),
            )
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
    }
}
