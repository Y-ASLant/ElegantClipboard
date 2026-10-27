use super::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::status_bar::StatusBar;

impl ClipboardView {
    pub(super) fn set_feedback_message(&mut self, message: String) {
        self.message = message;
        self.feedback_pending = true;
    }

    pub(super) fn schedule_feedback(main_window: AnyWindowHandle, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        // Settings reads the same view, so its implicit window association is not stable.
        // Defer until the observer releases the view before updating the explicit main window.
        cx.defer(move |cx| {
            let _ = main_window.update(cx, |_, window, cx| {
                owner.update(cx, |this, cx| this.show_feedback(window, cx))
            });
        });
    }

    fn show_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.feedback_pending) || self.message.is_empty() {
            return;
        }
        let notification = || {
            if self.is_error {
                Notification::error(self.message.clone())
            } else {
                Notification::success(self.message.clone())
            }
        };
        // A single destination prevents settings operations from notifying both windows.
        if let Some(handle) = self.settings_window
            && !window.is_window_active()
            && handle
                .update(cx, |_, window, cx| {
                    window.push_notification(notification().id::<ClipboardView>(), cx);
                })
                .is_ok()
        {
            return;
        }
        window.push_notification(notification().id::<ClipboardView>(), cx);
    }

    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        if !self.reorder_pending {
            return div().into_any_element();
        }
        StatusBar::new()
            .flex_shrink_0()
            .px(px(PAGE_PADDING))
            .left(div().text_color(cx.theme().muted_foreground).child(tr(
                self.language,
                "正在保存顺序…",
                "Saving order…",
            )))
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
        let drag_request = if self.card_drag_active(cx) {
            self.drag_source_id().zip(self.drop_target)
        } else if self.reorder_pending {
            self.drag_motion.request
        } else {
            None
        };
        self.drag_motion
            .update(drag_request, &self.history.items, &self.row_offsets);
        window_shell(cx)
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
            .when_some(crate::capabilities::desktop_unavailable_reason(), |shell, reason| {
                // Wayland 降级说明仅存在于 Linux；macOS 的辅助功能提示不应附带它。
                let wayland_note = if cfg!(target_os = "linux") {
                    "\nWayland：支持文件和图片拖出、窗口内排序；不支持文字/富文本拖出。关闭窗口时使用最小化。\nWayland: file/image export and in-app reordering work; text/rich-text export is unavailable. Hide minimizes the window."
                } else {
                    ""
                };
                shell.child(Alert::warning("desktop-capabilities", format!(
                    "{reason}{wayland_note}"
                )).small())
            })
            .child(div().flex_1().min_h_0().child(self.render_content(cx)))
            .child(self.render_status(cx))
    }
}
