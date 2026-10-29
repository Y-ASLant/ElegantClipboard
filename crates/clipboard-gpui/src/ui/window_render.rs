use super::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::status_bar::StatusBar;

impl ClipboardView {
    pub(super) fn set_feedback_message(&mut self, message: String) {
        self.message = message;
        self.feedback_pending = true;
    }

    pub(super) fn schedule_feedback(cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        // 等观察者释放视图借用后，再投递设置或系统通知。
        cx.defer(move |cx| {
            let _ = owner.update(cx, |this, cx| this.show_feedback(cx));
        });
    }

    fn show_feedback(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.feedback_pending) || self.message.is_empty() {
            return;
        }
        if let Some(handle) = self.settings_window {
            let notification = if self.is_error {
                Notification::error(self.message.clone())
            } else {
                Notification::success(self.message.clone())
            };
            if handle
                .update(cx, |_, window, cx| {
                    window.push_notification(notification.id::<ClipboardView>(), cx);
                })
                .is_ok()
            {
                return;
            }
        }
        let message = self.message.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) =
                    clipboard_platform::notification::show("ElegantClipboard", &message)
                {
                    // 系统禁用通知或投递失败时不递归通知，也不回退到应用内 toast。
                    tracing::error!(error = %format_args!("{error:#}"), "Sending system notification failed");
                }
            })
            .detach();
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
            .when_some(crate::capabilities::paste_unavailable_reason(self.language), |shell, reason| {
                let wayland_note = if crate::capabilities::wayland_session() {
                    tr(self.language,
                        "\nWayland：支持文件和图片拖出、窗口内排序；不支持文字/富文本拖出。关闭窗口时使用最小化。",
                        "\nWayland: file/image export and in-app reordering work; text/rich-text export is unavailable. Hide minimizes the window.")
                } else {
                    ""
                };
                shell.child(Alert::warning("desktop-capabilities", format!(
                    "{reason}{wayland_note}"
                )).small())
            })
            .child(div().flex_1().min_h_0().child(self.render_content(window, cx)))
            .child(self.render_status(cx))
    }
}
