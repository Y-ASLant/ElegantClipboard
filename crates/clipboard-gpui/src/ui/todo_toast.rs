use super::*;
use std::time::Duration;

const CARD_WIDTH: f32 = 320.;
const CARD_HEIGHT: f32 = 44.;
const SCREEN_MARGIN: f32 = 16.;
const VISIBLE_FOR: Duration = Duration::from_secs(3);

pub(super) struct TodoToastWindow {
    message: SharedString,
    close_label: &'static str,
    generation: u64,
    motion: u64,
    leaving: bool,
}

impl TodoToastWindow {
    pub(super) fn replace(&mut self, message: String, window: &mut Window, cx: &mut Context<Self>) {
        self.message = message.into();
        self.arm(window, cx);
        cx.notify();
    }

    fn arm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leaving = false;
        self.generation = self.generation.wrapping_add(1);
        self.motion = self.motion.wrapping_add(1);
        let generation = self.generation;
        let handle = window.window_handle();
        cx.spawn(async move |view, cx| {
            cx.background_executor().timer(VISIBLE_FOR).await;
            cx.update(|cx| {
                let _ = handle.update(cx, |_, toast_window, cx| {
                    let _ = view.update(cx, |toast, cx| {
                        if toast.generation == generation && !toast.leaving {
                            toast.dismiss(toast_window, cx);
                        }
                    });
                });
            });
        })
        .detach();
        cx.notify();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.leaving {
            return;
        }
        self.leaving = true;
        self.motion = self.motion.wrapping_add(1);
        if cx.reduce_motion() {
            window.remove_window();
            return;
        }
        let motion = self.motion;
        let handle = window.window_handle();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(visual::MOTION_DURATION)
                .await;
            cx.update(|cx| {
                let _ = handle.update(cx, |_, toast_window, cx| {
                    let _ = view.update(cx, |toast, _| {
                        if toast.leaving && toast.motion == motion {
                            toast_window.remove_window();
                        }
                    });
                });
            });
        })
        .detach();
        cx.notify();
    }
}

pub(super) fn open(
    message: String,
    close_label: &'static str,
    theme: ThemePreference,
    window: &mut Window,
    cx: &mut App,
) -> Option<WindowHandle<TodoToastWindow>> {
    let screen = window
        .display(cx)
        .map_or_else(|| window.bounds(), |display| display.visible_bounds());
    let scale = window.scale_factor();
    let display_id = window.display(cx).map(|display| display.id());
    let width = px(CARD_WIDTH).min(screen.size.width - px(SCREEN_MARGIN * 2.));
    let card_height = px(CARD_HEIGHT).min(screen.size.height - px(SCREEN_MARGIN * 2.));
    // Four extra pixels sit in the screen margin so the card can rise without clipping.
    let height = card_height + px(visual::SPACE_1);
    let origin = point(
        (screen.right() - width - px(SCREEN_MARGIN)).max(screen.left()),
        (screen.bottom() - card_height - px(SCREEN_MARGIN)).max(screen.top()),
    );
    let bounds = Bounds::new(origin, size(width, height));
    let opened = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id,
                // No reserved titlebar: a 44px popup otherwise keeps a blank white bar.
                titlebar: None,
                kind: WindowKind::PopUp,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: true,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_min_size: Some(size(px(1.), px(1.))),
                inactive_frame_interval: None,
                app_id: Some("com.aslant.elegant-clipboard-gpui.todo-toast".into()),
                ..Default::default()
            },
            move |window, cx| {
                apply_theme(theme, window, cx);
                window.refresh();
                cx.new(|_| TodoToastWindow {
                    message: message.into(),
                    close_label,
                    generation: 0,
                    motion: 0,
                    leaving: false,
                })
            },
        )
        .ok()?;
    let native = opened
        .update(cx, |toast, toast_window, cx| {
            toast.arm(toast_window, cx);
            toast_window.refresh();
            native_window::window_id(toast_window)
        })
        .ok()
        .flatten()?;
    place(native, bounds, scale);
    Some(opened)
}

fn place(native: isize, bounds: Bounds<Pixels>, scale: f32) {
    #[cfg(windows)]
    {
        let _ = crate::native::preview::avoid_activation(native);
        let _ = crate::native::preview::set_bounds(native, bounds, scale);
    }
    #[cfg(not(windows))]
    {
        let _ = clipboard_platform::external_drag::resize_drag_preview(
            native,
            (f32::from(bounds.size.width) * scale).round() as i32,
            (f32::from(bounds.size.height) * scale).round() as i32,
        );
        let _ = clipboard_platform::external_drag::move_drag_preview(
            native,
            (f32::from(bounds.left()) * scale).round() as i32,
            (f32::from(bounds.top()) * scale).round() as i32,
        );
    }
}

impl Render for TodoToastWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(not(target_os = "macos"))]
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        let close_label = self.close_label;
        let motion = self.motion;
        let leaving = self.leaving;
        let card = div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(CARD_HEIGHT))
            .flex()
            .items_center()
            .gap(px(visual::SPACE_2))
            .px(px(visual::SPACE_3))
            .overflow_hidden()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .border(px(visual::BORDER_WIDTH))
            .border_color(cx.theme().border)
            .text_size(px(visual::TEXT_BODY))
            .text_color(cx.theme().foreground)
            .child(
                img("app-icon.png")
                    .size(px(18.))
                    .flex_shrink_0()
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.message.clone()),
            )
            .child(
                Button::new("todo-toast-close")
                    .ghost()
                    .xsmall()
                    .flex_shrink_0()
                    .icon(Icon::new(IconName::Close).size(px(visual::ICON_SMALL)))
                    .accessibility_label(close_label)
                    .tooltip(close_label)
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx))),
            );
        let card = if cx.reduce_motion() || motion == 0 {
            card.top_0()
                .when(motion == 0, |card| card.opacity(0.))
                .into_any_element()
        } else {
            let exit = leaving;
            card.with_animation(
                ("todo-toast", motion),
                toast_motion(cx, exit),
                move |card, progress| {
                    let (offset, opacity) = toast_shift(progress, exit);
                    card.top(px(offset)).opacity(opacity)
                },
            )
            .into_any_element()
        };
        div().relative().size_full().child(card)
    }
}

fn toast_motion(cx: &App, exit: bool) -> Animation {
    let easing = if exit {
        cx.theme().motion.easing_exit.clone()
    } else {
        cx.theme().motion.easing_enter.clone()
    };
    Animation::new(visual::MOTION_DURATION).with_easing(move |progress| easing.sample(progress))
}

fn toast_shift(progress: f32, leaving: bool) -> (f32, f32) {
    if leaving {
        (visual::SPACE_1 * progress, 1. - progress)
    } else {
        (visual::SPACE_1 * (1. - progress), progress)
    }
}

#[cfg(test)]
mod tests {
    use super::toast_shift;

    #[test]
    fn toast_rises_in_and_drops_out() {
        assert_eq!(toast_shift(0., false), (4., 0.));
        assert_eq!(toast_shift(1., false), (0., 1.));
        assert_eq!(toast_shift(0., true), (0., 1.));
        assert_eq!(toast_shift(1., true), (4., 0.));
    }
}
