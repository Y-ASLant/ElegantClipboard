use super::*;
use gpui_kit::Size as Extent;
use gpui_kit::component::menu::PopupMenu;

use std::cell::RefCell;

const MENU_SLOT: f32 = 328.;
const MENU_HEIGHT: f32 = 480.;
const MENU_SHADOW: f32 = 8.;
const SUBMENU_ROOM: f32 = 320.;
// PopupMenu rows are rems(1.625), with p_1 padding and gap_y_0p5.
const MENU_ITEM_REM: f32 = 1.625;
const MENU_PAD_REM: f32 = 0.25;
const MENU_GAP_REM: f32 = 0.125;

type MenuBuild = Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;
type PlacedKey = (i32, i32, i32, i32);

pub(super) struct HistoryMenuWindow {
    menu: Option<Entity<PopupMenu>>,
    build: Option<MenuBuild>,
    armed: bool,
    shown: Rc<Cell<bool>>,
    flyout: Rc<Cell<bool>>,
    rows: Rc<RefCell<Vec<RowKind>>>,
    menu_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    anchor: Point<Pixels>,
    screen: Bounds<Pixels>,
    fitted: Rc<Cell<Option<PlacedKey>>>,
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
            view.close_hover_preview("history_menu_opening", cx);
            if let Some(existing) = view.history_menu_window.take() {
                let _ = existing.update(cx, |_, window, _| window.remove_window());
            }
        });
        let cursor = window.bounds().origin + position;
        let screen = window
            .display(cx)
            .map_or(window.bounds(), |display| display.visible_bounds());
        // Hidden until the first layout. One slot is enough to measure the
        // menu; the window is resized to that size before it is shown.
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
        let release_owner = weak.clone();
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
                app_id: Some("com.aslant.ElegantClipboard.menu".into()),
                ..Default::default()
            },
            {
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
                        window_lifecycle::bind_satellite_release(release_owner, window, cx);
                        HistoryMenuWindow {
                            menu: None,
                            build: Some(Box::new(build)),
                            armed,
                            shown: Rc::new(Cell::new(false)),
                            flyout: Rc::new(Cell::new(false)),
                            rows: Rc::new(RefCell::new(Vec::new())),
                            menu_bounds: Rc::new(Cell::new(None)),
                            anchor: cursor,
                            screen,
                            fitted: Rc::new(Cell::new(None)),
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

#[cfg(target_os = "macos")]
fn place_menu_window(native: isize, bounds: Bounds<Pixels>, _scale: f32) {
    native_window::prepare_menu_window(native);
    // AppKit takes logical points, including on Retina displays. Resize first
    // because setContentSize can move the top edge of the native window.
    let result = clipboard_platform::external_drag::resize_drag_preview(
        native,
        f32::from(bounds.size.width).round() as i32,
        f32::from(bounds.size.height).round() as i32,
    )
    .and_then(|()| {
        clipboard_platform::external_drag::move_drag_preview(
            native,
            f32::from(bounds.left()).round() as i32,
            f32::from(bounds.top()).round() as i32,
        )
    });
    if let Err(error) = result {
        tracing::warn!(%error, "failed to position macOS menu window");
    }
}

#[cfg(not(target_os = "macos"))]
fn place_menu_window(native: isize, bounds: Bounds<Pixels>, scale: f32) {
    #[cfg(windows)]
    let _ = crate::native::preview::suppress_activation(native);
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
}

fn close_menu_window(_: &MouseDownEvent, window: &mut Window, cx: &mut App) {
    cx.stop_propagation();
    window.remove_window();
}

// PopupMenu reads theme.radius while it renders, which is after this function
// returns but still inside the window update. global_mut skips Theme::update,
// whose refresh would draw every window again. The deferred restore runs when
// that update ends, after this window's draw.
fn use_card_radius(cx: &mut Context<HistoryMenuWindow>) {
    let radius = crate::card_radius::card_radius(cx);
    let previous = gpui_kit::component::Theme::global(cx).radius;
    if previous == radius {
        return;
    }
    gpui_kit::component::Theme::global_mut(cx).radius = radius;
    cx.defer(move |cx| {
        gpui_kit::component::Theme::global_mut(cx).radius = previous;
    });
}

// Without this width the window clips the submenu painted past the menu's right edge.
fn fitted_menu_bounds(
    anchor: Point<Pixels>,
    screen: Bounds<Pixels>,
    menu: Extent<Pixels>,
    flyout: bool,
) -> Bounds<Pixels> {
    let pad = px(MENU_SHADOW);
    let width = if flyout {
        menu.width + pad + px(SUBMENU_ROOM) + px(1.)
    } else {
        menu.width + pad * 2.
    };
    let width = width.min(screen.size.width).max(px(1.));
    let height = (menu.height + pad * 2.).min(screen.size.height).max(px(1.));
    let mut origin_x = anchor.x - pad;
    let mut origin_y = anchor.y - pad;
    if origin_y + height > screen.bottom() {
        origin_y = anchor.y - height + pad;
    }
    if origin_x + width > screen.right() {
        origin_x = screen.right() - width;
    }
    origin_x = origin_x.max(screen.left());
    origin_y = origin_y.max(screen.top());
    Bounds::new(point(origin_x, origin_y), size(width, height))
}

#[derive(Clone, Copy)]
pub(super) enum RowKind {
    Item,
    Submenu,
    Separator,
}

thread_local! {
    static MENU_ROWS: Cell<Option<Rc<RefCell<Vec<RowKind>>>>> = const { Cell::new(None) };
}

pub(super) fn note_menu_row(kind: RowKind) {
    MENU_ROWS.with(|current| {
        if let Some(rows) = current.take() {
            rows.borrow_mut().push(kind);
            current.set(Some(rows));
        }
    });
}

fn pointer_wants_flyout(
    menu: Bounds<Pixels>,
    pointer: Point<Pixels>,
    rows: &[RowKind],
    rem: Pixels,
    open: bool,
) -> bool {
    let item = rem * MENU_ITEM_REM;
    let gap = rem * MENU_GAP_REM;
    let pad = rem * MENU_PAD_REM;
    let mut y = menu.top() + pad;
    let mut on_submenu = false;
    let mut on_other = false;
    for row in rows {
        let advance = match row {
            RowKind::Separator => rem * MENU_GAP_REM * 2. + px(2.) + gap,
            RowKind::Item | RowKind::Submenu => {
                if pointer.y >= y
                    && pointer.y < y + item
                    && pointer.x >= menu.left()
                    && pointer.x <= menu.right()
                {
                    on_submenu |= matches!(row, RowKind::Submenu);
                    on_other |= matches!(row, RowKind::Item);
                }
                item + gap
            }
        };
        y += advance;
    }
    // The kit keeps a submenu open until another row is hovered. Shrinking
    // before that paints the submenu back over the parent.
    if on_submenu {
        return true;
    }
    if on_other {
        return false;
    }
    open
}

fn placed_key(bounds: Bounds<Pixels>) -> PlacedKey {
    (
        f32::from(bounds.origin.x).round() as i32,
        f32::from(bounds.origin.y).round() as i32,
        f32::from(bounds.size.width).round() as i32,
        f32::from(bounds.size.height).round() as i32,
    )
}

impl HistoryMenuWindow {
    fn sync_flyout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown.get() {
            return;
        }
        let Some(menu) = self.menu_bounds.get() else {
            return;
        };
        let wants = pointer_wants_flyout(
            menu,
            window.mouse_position(),
            &self.rows.borrow(),
            window.rem_size(),
            self.flyout.get(),
        );
        if wants == self.flyout.get() {
            return;
        }
        let Some(native) = native_window::window_id(window) else {
            return;
        };
        let placed = fitted_menu_bounds(self.anchor, self.screen, menu.size, wants);
        place_menu_window(native, placed, window.scale_factor());
        window.bounds_changed(cx);
        // Refreshes requested while drawing are dropped, so the frame presented on
        // the resized surface would stay stretched until the next unrelated redraw.
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.bounds_changed(cx));
        });
        self.fitted.set(Some(placed_key(placed)));
        self.flyout.set(wants);
    }
}

impl Render for HistoryMenuWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // On macOS, keep the clear native background prepared before showing.
        // GPUI's Transparent setter restores a nonzero alpha for window shadows.
        #[cfg(not(target_os = "macos"))]
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);
        use_card_radius(cx);
        // This standalone menu has no Kit Root to apply the interface font scale.
        window.set_rem_size(cx.theme().font_size);
        if self.menu.is_none()
            && let Some(build) = self.build.take()
        {
            let rows = self.rows.clone();
            MENU_ROWS.with(|current| current.set(Some(rows)));
            self.menu = Some(PopupMenu::build(window, cx, move |menu, window, cx| {
                build(menu, window, cx)
            }));
            MENU_ROWS.with(|current| current.set(None));
        }
        self.sync_flyout(window, cx);
        let menu = self.menu.clone();
        let anchor = self.anchor;
        let screen = self.screen;
        let fitted = self.fitted.clone();
        let flyout = self.flyout.clone();
        let menu_bounds = self.menu_bounds.clone();
        let shown = self.shown.clone();
        div()
            .size_full()
            .occlude()
            .flex()
            .flex_col()
            .items_start()
            .font_family(cx.theme().font_family.clone())
            .on_mouse_down(MouseButton::Left, close_menu_window)
            .on_mouse_down(MouseButton::Right, close_menu_window)
            .when_some(menu, |layer, menu| {
                layer.child(
                    div().m(px(MENU_SHADOW)).max_w(px(320.)).child(menu).child(
                        canvas(
                            move |bounds, window, cx| {
                                if bounds.size.width < px(1.) || bounds.size.height < px(1.) {
                                    return;
                                }
                                menu_bounds.set(Some(bounds));
                                let placed =
                                    fitted_menu_bounds(anchor, screen, bounds.size, flyout.get());
                                let Some(native) = native_window::window_id(window) else {
                                    return;
                                };
                                let key = placed_key(placed);
                                if fitted.get() == Some(key) {
                                    return;
                                }
                                fitted.set(Some(key));
                                let scale = window.scale_factor();
                                let reveal = !shown.get();
                                shown.set(true);
                                cx.defer(move |_| {
                                    place_menu_window(native, placed, scale);
                                    if reveal {
                                        let _ =
                                            clipboard_platform::external_drag::show_drag_preview(
                                                native,
                                            );
                                    }
                                });
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

#[cfg(test)]
mod tests {
    use super::{MENU_SHADOW, RowKind, fitted_menu_bounds, pointer_wants_flyout};
    use gpui_kit::{Bounds, Pixels, point, px, size};

    fn screen() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(1200.), px(800.)))
    }

    #[test]
    fn closed_menu_window_matches_the_menu_not_a_fixed_slot() {
        let menu = size(px(240.), px(300.));
        let anchor = point(px(400.), px(200.));
        let bounds = fitted_menu_bounds(anchor, screen(), menu, false);
        assert_eq!(bounds.size, size(px(256.), px(316.)));
        assert_eq!(bounds.origin, point(px(392.), px(192.)));
    }

    #[test]
    fn menu_near_the_bottom_flips_above_the_cursor() {
        let menu = size(px(240.), px(120.));
        let anchor = point(px(100.), px(760.));
        let bounds = fitted_menu_bounds(anchor, screen(), menu, false);
        assert_eq!(bounds.bottom(), anchor.y + px(MENU_SHADOW));
    }

    #[test]
    fn open_flyout_includes_the_submenu() {
        let menu = size(px(240.), px(300.));
        let anchor = point(px(400.), px(200.));
        let bounds = fitted_menu_bounds(anchor, screen(), menu, true);
        assert_eq!(bounds.size, size(px(569.), px(316.)));
        assert_eq!(bounds.origin, point(px(392.), px(192.)));
    }

    #[test]
    fn flyout_stays_closed_until_the_pointer_is_on_its_row() {
        let menu = Bounds::new(point(px(0.), px(0.)), size(px(240.), px(300.)));
        let rows = [RowKind::Item, RowKind::Item, RowKind::Submenu];
        let rem = px(16.);
        assert!(pointer_wants_flyout(
            menu,
            point(px(20.), px(70.)),
            &rows,
            rem,
            false,
        ));
        assert!(!pointer_wants_flyout(
            menu,
            point(px(20.), px(40.)),
            &rows,
            rem,
            false,
        ));
        assert!(pointer_wants_flyout(
            menu,
            point(px(300.), px(100.)),
            &rows,
            rem,
            true,
        ));
        assert!(!pointer_wants_flyout(
            menu,
            point(px(300.), px(100.)),
            &rows,
            rem,
            false,
        ));
        assert!(!pointer_wants_flyout(
            menu,
            point(px(20.), px(40.)),
            &rows,
            rem,
            true,
        ));
        assert!(pointer_wants_flyout(
            menu,
            point(px(20.), px(200.)),
            &rows,
            rem,
            true,
        ));
    }

    #[test]
    fn row_after_a_separator_closes_the_flyout() {
        let menu = Bounds::new(point(px(0.), px(0.)), size(px(240.), px(300.)));
        let rows = [
            RowKind::Item,
            RowKind::Submenu,
            RowKind::Separator,
            RowKind::Item,
        ];
        let rem = px(16.);
        assert!(!pointer_wants_flyout(
            menu,
            point(px(20.), px(90.)),
            &rows,
            rem,
            true,
        ));
    }
}
