use super::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::status_bar::StatusBar;
use std::time::Duration;

const PANEL_STATUS_OK_FOR: Duration = Duration::from_secs(3);
const PANEL_STATUS_ERR_FOR: Duration = Duration::from_secs(6);

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
        // Keep the line on the main StatusBar; auto-clear so it does not stick forever.
        self.arm_panel_status_clear(cx);
        cx.notify();

        if let Some(handle) = self.settings_window {
            let notification = feedback_notification(self.message.clone(), self.is_error);
            if handle
                .update(cx, |_, window, cx| {
                    window.push_notification(notification.id::<ClipboardView>(), cx);
                })
                .is_ok()
            {
                return;
            }
        }

        // Main panel already shows the line; skip OS toast while it is visible.
        let panel_visible = self
            .main_window
            .update(cx, |_, window, _| {
                native_window::is_window_shown(window)
                    && !native_window::is_window_minimized(window)
            })
            .unwrap_or(false);
        if panel_visible {
            return;
        }

        let message = self.message.clone();
        cx.background_executor()
            .spawn(async move {
                if let Err(error) =
                    clipboard_platform::notification::show("ElegantClipboard", &message)
                {
                    // 系统禁用通知或投递失败时不递归通知。
                    tracing::error!(error = %format_args!("{error:#}"), "Sending system notification failed");
                }
            })
            .detach();
    }

    fn arm_panel_status_clear(&mut self, cx: &mut Context<Self>) {
        self.panel_status_token = self.panel_status_token.wrapping_add(1);
        let token = self.panel_status_token;
        let wait = if self.is_error {
            PANEL_STATUS_ERR_FOR
        } else {
            PANEL_STATUS_OK_FOR
        };
        self.panel_status_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(wait).await;
            let _ = view.update(cx, |this, cx| {
                if this.panel_status_token == token {
                    this.message.clear();
                    cx.notify();
                }
            });
        }));
    }

    fn render_status(&self, cx: &Context<Self>) -> AnyElement {
        let has_message = !self.message.is_empty();
        if !self.reorder_pending && !has_message {
            return div().into_any_element();
        }
        let left = if has_message {
            let color = if self.is_error {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            };
            div()
                .min_w_0()
                .text_color(color)
                .child(self.message.clone())
        } else {
            div().text_color(cx.theme().muted_foreground).child(tr(
                self.language,
                "正在保存顺序…",
                "Saving order…",
            ))
        };
        StatusBar::new()
            .flex_shrink_0()
            .px(px(PAGE_PADDING))
            .left(left)
            .into_any_element()
    }
}

// A typed Notification pins its icon at top: 18px, so the mark and the line
// box stop sharing a center. Build the row here instead.
fn feedback_notification(message: impl Into<SharedString>, error: bool) -> Notification {
    let message = message.into();
    Notification::new().content(move |_, _, cx| {
        let (icon, color) = if error {
            (IconName::CircleX, cx.theme().danger)
        } else {
            (IconName::CircleCheck, cx.theme().success)
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .line_height(relative(1.))
            .child(Icon::new(icon).size_4().flex_shrink_0().text_color(color))
            .child(div().min_w_0().child(message.clone()))
            .into_any_element()
    })
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
        window_shell(cx)
            .child(history_menu_dismiss_layer(cx))
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_content(window, cx)),
            )
            .child(self.render_status(cx))
    }
}

fn history_menu_dismiss_layer(cx: &mut Context<ClipboardView>) -> impl IntoElement {
    let owner = cx.entity().downgrade();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let owner = owner.clone();
            window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let Some(owner) = owner.upgrade() else {
                    return;
                };
                owner.update(cx, |this, cx| {
                    this.close_history_menu(cx);
                });
            });
        },
    )
    .absolute()
    .size(px(0.))
}
