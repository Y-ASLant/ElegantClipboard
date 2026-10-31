use super::presentation::apply_theme;
use super::*;
use gpui_kit::component::menu::PopupMenu;

const MENU_SLOT: f32 = 328.;
const MENU_HEIGHT: f32 = 480.;

type MenuBuild = Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

pub(super) struct HistoryMenuWindow {
    owner: WeakEntity<ClipboardView>,
    menu: Option<Entity<PopupMenu>>,
    build: Option<MenuBuild>,
    armed: bool,
    _activation: Subscription,
}

impl HistoryMenuWindow {
    pub(super) fn open(
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
        owner: Entity<ClipboardView>,
        build: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) {
        owner.update(cx, |view, cx| {
            if let Some(existing) = view.history_menu_window.take() {
                let _ = existing.update(cx, |_, window, _| window.remove_window());
            }
        });
        let cursor = window.bounds().origin + position;
        let screen = window
            .display(cx)
            .map_or(window.bounds(), |display| display.visible_bounds());
        let width = px(MENU_SLOT).min(screen.size.width);
        let height = px(MENU_HEIGHT).min(screen.size.height);
        let origin_x = cursor
            .x
            .clamp(screen.left(), (screen.right() - width).max(screen.left()));
        let mut origin_y = cursor.y;
        if origin_y + height > screen.bottom() {
            origin_y = (screen.bottom() - height).max(screen.top());
        }
        let display_id = window.display(cx).map(|display| display.id());
        let weak = owner.downgrade();
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(origin_x, origin_y),
                    size(width, height),
                ))),
                display_id,
                titlebar: None,
                kind: WindowKind::PopUp,
                window_background: WindowBackgroundAppearance::Transparent,
                focus: false,
                show: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_min_size: Some(size(px(1.), px(1.))),
                app_id: Some("com.aslant.elegant-clipboard-gpui.menu".into()),
                ..Default::default()
            },
            {
                let owner = weak.clone();
                move |window, cx| {
                    cx.new(|cx| {
                        let armed = window.is_window_active();
                        let _activation = cx.observe_window_activation(
                            window,
                            |this: &mut HistoryMenuWindow, window, _| {
                                if window.is_window_active() {
                                    this.armed = true;
                                } else if this.armed {
                                    window.remove_window();
                                }
                            },
                        );
                        HistoryMenuWindow {
                            owner,
                            menu: None,
                            build: Some(Box::new(build)),
                            armed,
                            _activation,
                        }
                    })
                }
            },
        );
        if let Ok(opened) = opened {
            let bounds = Bounds::new(point(origin_x, origin_y), size(width, height));
            let scale = window.scale_factor();
            let native = opened
                .update(cx, |_, menu_window, _| {
                    native_window::window_id(menu_window)
                })
                .ok()
                .flatten();
            if let Some(native) = native {
                place_menu_window(native, bounds, scale);
            }
            if let Some(owner) = weak.upgrade() {
                owner.update(cx, |view, _| view.history_menu_window = Some(opened.into()));
            }
        }
    }
}

fn place_menu_window(native: isize, bounds: Bounds<Pixels>, scale: f32) {
    #[cfg(windows)]
    let _ = crate::native::preview::set_bounds(native, bounds, scale);
    #[cfg(not(windows))]
    {
        let _ = clipboard_platform::external_drag::move_drag_preview(
            native,
            (f32::from(bounds.left()) * scale).round() as i32,
            (f32::from(bounds.top()) * scale).round() as i32,
        );
        let _ = clipboard_platform::external_drag::resize_drag_preview(
            native,
            (f32::from(bounds.size.width) * scale).round() as i32,
            (f32::from(bounds.size.height) * scale).round() as i32,
        );
    }
    let _ = clipboard_platform::external_drag::show_drag_preview(native);
}

impl Render for HistoryMenuWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        if self.menu.is_none() {
            if let Some(owner) = self.owner.upgrade() {
                let (theme, color) = {
                    let owner = owner.read(cx);
                    (owner.theme, owner.theme_color)
                };
                apply_theme(theme, window, cx);
                window.set_background_appearance(WindowBackgroundAppearance::Transparent);
                crate::design_system::set_theme_color(color, cx);
            }
            if let Some(build) = self.build.take() {
                self.menu = Some(PopupMenu::build(window, cx, move |menu, window, cx| {
                    build(menu, window, cx)
                }));
            }
        }
        let menu = self.menu.clone();
        div()
            .size_full()
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                cx.stop_propagation();
                window.remove_window();
            })
            .on_mouse_down(MouseButton::Right, |_, window, cx| {
                cx.stop_propagation();
                window.remove_window();
            })
            .when_some(menu, |layer, menu| {
                layer.child(
                    div().w(px(MENU_SLOT)).child(menu).child(
                        canvas(
                            |bounds, window, _| {
                                if bounds.size.width < px(200.) || bounds.size.height < px(120.) {
                                    return;
                                }
                                let viewport = window.viewport_size();
                                if (viewport.width - bounds.size.width).abs() > px(1.)
                                    || (viewport.height - bounds.size.height).abs() > px(1.)
                                {
                                    window.resize(bounds.size);
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
                )
            })
    }
}
