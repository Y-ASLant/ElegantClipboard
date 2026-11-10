use super::*;

const BUTTON_SIZE: f32 = visual::CONTROL_HEIGHT_LARGE;
const INSET_X: f32 = visual::SPACE_4 + visual::SPACE_1;
const INSET_Y: f32 = visual::SPACE_4;
const DRAG_THRESHOLD: f32 = 4.;

/// Fractions of the available travel keep the docked control inside resized lists.
pub(super) struct ScrollTopPosition {
    x: f32,
    y: f32,
    gesture: Option<PointerGesture>,
    suppress_click: bool,
}

struct PointerGesture {
    pointer: Point<Pixels>,
    origin: Point<Pixels>,
    moved: bool,
}

impl Default for ScrollTopPosition {
    fn default() -> Self {
        Self {
            x: 1.,
            y: 1.,
            gesture: None,
            suppress_click: false,
        }
    }
}

impl ScrollTopPosition {
    fn travel(viewport: Bounds<Pixels>) -> Point<Pixels> {
        point(
            (viewport.size.width - px(2. * INSET_X + BUTTON_SIZE)).max(px(0.)),
            (viewport.size.height - px(2. * INSET_Y + BUTTON_SIZE)).max(px(0.)),
        )
    }

    fn begin(&mut self, pointer: Point<Pixels>, viewport: Bounds<Pixels>) {
        let travel = Self::travel(viewport);
        self.gesture = Some(PointerGesture {
            pointer,
            origin: point(travel.x * self.x, travel.y * self.y),
            moved: false,
        });
        self.suppress_click = false;
    }

    fn move_to(&mut self, pointer: Point<Pixels>, viewport: Bounds<Pixels>) {
        let Some(gesture) = &mut self.gesture else {
            return;
        };
        let delta = pointer - gesture.pointer;
        let dx = f32::from(delta.x);
        let dy = f32::from(delta.y);
        gesture.moved |= dx * dx + dy * dy >= DRAG_THRESHOLD * DRAG_THRESHOLD;
        if !gesture.moved {
            return;
        }
        let travel = Self::travel(viewport);
        self.x = Self::fraction(gesture.origin.x + delta.x, travel.x);
        self.y = Self::fraction(gesture.origin.y + delta.y, travel.y);
    }

    fn fraction(position: Pixels, extent: Pixels) -> f32 {
        if extent > px(0.) {
            (f32::from(position) / f32::from(extent)).clamp(0., 1.)
        } else {
            0.
        }
    }

    /// 结束本次按压。返回是否应视为点击。拖拽只抑制同一次松手产生的按钮点击，不挡住下一次。
    pub(super) fn finish(&mut self) -> bool {
        let Some(gesture) = self.gesture.take() else {
            return false;
        };
        let clicked = !gesture.moved;
        self.suppress_click = gesture.moved;
        if gesture.moved {
            self.x = if self.x < 0.5 { 0. } else { 1. };
        }
        clicked
    }

    pub(super) fn take_suppressed_click(&mut self) -> bool {
        let suppressed = self.suppress_click;
        self.suppress_click = false;
        suppressed
    }

    pub(super) fn is_pressed(&self) -> bool {
        self.gesture.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FloatingAction {
    ScrollTop,
    AddTodo,
}

impl ClipboardView {
    pub(super) fn activate_floating_action(
        &mut self,
        action: FloatingAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            FloatingAction::ScrollTop => self.animate_scroll_to_top(cx),
            FloatingAction::AddTodo => self.open_todo_editor(None, String::new(), window, cx),
        }
        cx.notify();
    }

    pub(super) fn render_floating_action(
        &self,
        action: FloatingAction,
        cx: &mut Context<Self>,
    ) -> Div {
        let owner = cx.entity().downgrade();
        let position = &self.scroll_top_position;
        let label = match action {
            FloatingAction::ScrollTop => tr(self.language, "返回顶部", "Back to top"),
            FloatingAction::AddTodo => tr(self.language, "添加待办", "Add to-do"),
        };
        div()
            .absolute()
            .left(px(INSET_X))
            .right(px(INSET_X))
            .top(px(INSET_Y))
            .bottom(px(INSET_Y))
            .child(
                div()
                    .id("history-scroll-top-float")
                    .absolute()
                    .left(relative(position.x))
                    .top(relative(position.y))
                    .ml(px(-BUTTON_SIZE * position.x))
                    .mt(px(-BUTTON_SIZE * position.y))
                    .size(px(BUTTON_SIZE))
                    // Only the floating control blocks row hits, not its full-list positioning layer.
                    .block_mouse_except_scroll()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().popover)
                    .shadow(visual::control_shadow())
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.close_hover_preview("scroll_top_hovered", cx);
                        }
                    }))
                    .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if event.button != MouseButton::Left {
                            return;
                        }
                        // 上次拖拽留下的抑制不能挡住这次按下。
                        this.scroll_top_position.suppress_click = false;
                        if let Some(viewport) = this.drag_viewport.get() {
                            this.scroll_top_position.begin(event.position, viewport);
                            this.cancel_pending_row_click();
                            this.close_hover_preview("scroll_top_pressed", cx);
                        }
                    }))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Button::new(match action {
                            FloatingAction::ScrollTop => "history-scroll-to-top",
                            FloatingAction::AddTodo => "todo-add-float",
                        })
                        .outline()
                        .size(px(BUTTON_SIZE))
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().popover)
                        .border_color(cx.theme().border)
                        .icon(
                            Icon::new(match action {
                                FloatingAction::ScrollTop => {
                                    gpui_kit::assets::IconName::ArrowUpToLine
                                }
                                FloatingAction::AddTodo => gpui_kit::assets::IconName::Plus,
                            })
                            .size(px(visual::ICON_SIZE)),
                        )
                        .tooltip(label)
                        .accessibility_label(label)
                        .disabled(
                            action == FloatingAction::AddTodo
                                && (self.todo_edit_saving
                                    || self.editor_window.is_some()
                                    || self.todo_draft.is_some()),
                        )
                        .on_click(cx.listener(
                            move |this, event, window, cx| {
                                cx.stop_propagation();
                                if !matches!(event, ClickEvent::Keyboard(_))
                                    && this.scroll_top_position.take_suppressed_click()
                                {
                                    return;
                                }
                                this.activate_floating_action(action, window, cx);
                            },
                        )),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, (), window, _| {
                        let movement_owner = owner.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let Some(owner) = movement_owner.upgrade() else {
                                return;
                            };
                            owner.update(cx, |this, cx| {
                                if !this.scroll_top_position.is_pressed() {
                                    return;
                                }
                                if event.pressed_button == Some(MouseButton::Left) {
                                    if let Some(viewport) = this.drag_viewport.get() {
                                        this.scroll_top_position.move_to(event.position, viewport);
                                    }
                                } else {
                                    // Recover if release happened outside the native window.
                                    this.scroll_top_position.finish();
                                }
                                cx.stop_propagation();
                                cx.notify();
                            });
                        });
                        let release_owner = owner.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || event.button != MouseButton::Left
                            {
                                return;
                            }
                            let Some(owner) = release_owner.upgrade() else {
                                return;
                            };
                            owner.update(cx, |this, cx| {
                                if this.scroll_top_position.is_pressed() {
                                    if let Some(viewport) = this.drag_viewport.get() {
                                        this.scroll_top_position.move_to(event.position, viewport);
                                    }
                                    let clicked = this.scroll_top_position.finish();
                                    if clicked {
                                        // 按钮的 on_click 可能在拖拽后不再送达，松手即执行未移动的点击。
                                        this.scroll_top_position.suppress_click = true;
                                        this.activate_floating_action(action, window, cx);
                                    }
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .size(px(0.)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{BUTTON_SIZE, INSET_X, INSET_Y, ScrollTopPosition};
    use gpui_kit::{Bounds, Pixels, point, px, size};

    fn viewport(width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(100.)), size(px(width), px(height)))
    }

    #[test]
    fn drag_follows_pointer_and_docks_without_becoming_a_click() {
        let bounds = viewport(400., 600.);
        let mut position = ScrollTopPosition::default();
        let start = point(px(362.), px(666.));
        position.begin(start, bounds);
        position.move_to(start + point(px(-250.), px(-200.)), bounds);
        assert!(position.x > 0. && position.x < 0.5);
        assert!((position.y - 332. / 532.).abs() < 0.0001);
        position.finish();
        assert_eq!(position.x, 0.);
        assert!(position.suppress_click);
        assert!(!position.is_pressed());
        position.begin(start, bounds);
        position.move_to(start + point(px(200.), px(0.)), bounds);
        position.finish();
        assert_eq!(position.x, 1.);
    }

    #[test]
    fn jitter_clicks_but_returning_after_a_drag_does_not() {
        let bounds = viewport(400., 600.);
        let start = point(px(100.), px(300.));
        let mut position = ScrollTopPosition::default();
        position.begin(start, bounds);
        position.move_to(start + point(px(2.), px(1.)), bounds);
        position.finish();
        assert!(!position.suppress_click);
        assert_eq!((position.x, position.y), (1., 1.));
        position.begin(start, bounds);
        position.move_to(start + point(px(-20.), px(-20.)), bounds);
        position.move_to(start, bounds);
        position.finish();
        assert!(position.suppress_click);
        position.begin(start, bounds);
        position.finish();
        assert!(!position.suppress_click);
    }

    #[test]
    fn outside_release_clamps_and_resize_preserves_the_docked_fraction() {
        let bounds = viewport(400., 600.);
        let start = point(px(362.), px(666.));
        let mut position = ScrollTopPosition::default();
        position.begin(start, bounds);
        position.move_to(point(px(-500.), px(-500.)), bounds);
        position.finish();
        assert_eq!((position.x, position.y), (0., 0.));
        position.begin(start, bounds);
        position.move_to(point(px(1500.), px(1500.)), bounds);
        position.finish();
        assert_eq!((position.x, position.y), (1., 1.));
        let travel = ScrollTopPosition::travel(viewport(300., 240.));
        assert_eq!(travel.x * position.x + px(INSET_X + BUTTON_SIZE), px(280.));
        assert_eq!(travel.y * position.y + px(INSET_Y + BUTTON_SIZE), px(224.));
        position.begin(start, viewport(40., 40.));
        position.move_to(point(px(0.), px(0.)), viewport(40., 40.));
        position.finish();
        assert!(position.x.is_finite() && position.y.is_finite());
    }
}
