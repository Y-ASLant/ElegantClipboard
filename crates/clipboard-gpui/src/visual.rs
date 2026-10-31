//! Shared spacing, control sizing and motion for the native UI.
use clipboard_core::preferences::CardDensity;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
// Logical-pixel scale shared by application layouts and official Kit controls.
pub const SPACE_1: f32 = 4.;
pub const SPACE_2: f32 = 8.;
pub const SPACE_3: f32 = 12.;
pub const SPACE_4: f32 = 16.;
pub const SPACE_6: f32 = 24.;
pub const CONTROL_HEIGHT: f32 = 32.;
pub const CONTROL_HEIGHT_LARGE: f32 = 36.;
pub const TEXT_SMALL: f32 = 12.;
pub const TEXT_BODY: f32 = 14.;
pub const TEXT_TITLE: f32 = 18.;
pub const ICON_SMALL: f32 = 14.;
pub const ICON_SIZE: f32 = 16.;
pub const BORDER_WIDTH: f32 = 1.;
pub const SCROLLBAR_RAIL_WIDTH: f32 = 12.;
pub const CHOICE_MAX_WIDTH: f32 = 640.;

pub fn popover_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.), px(2.), hsla(0., 0., 0., 0.2)).blur_radius(px(12.))]
}

pub fn control_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow::new(px(0.), px(1.), hsla(0., 0., 0., 0.12)).blur_radius(px(3.))]
}

pub const PAGE_PADDING: f32 = SPACE_3;
pub fn card_padding(density: CardDensity) -> f32 {
    match density {
        CardDensity::Compact => SPACE_2,
        CardDensity::Standard => 10.,
        CardDensity::Spacious => SPACE_3,
    }
}
pub const ROW_HEIGHT: f32 = 96.;
pub fn row_height(density: CardDensity, preview_lines: u8) -> f32 {
    let base = match density {
        CardDensity::Compact => 88.,
        CardDensity::Standard => ROW_HEIGHT,
        CardDensity::Spacious => 112.,
    };
    base + f32::from(preview_lines.saturating_sub(2)) * 20.
}

pub fn image_row_height(density: CardDensity) -> f32 {
    match density {
        CardDensity::Compact => 112.,
        CardDensity::Standard => 124.,
        CardDensity::Spacious => 148.,
    }
}

pub fn card_spacing(density: CardDensity) -> f32 {
    match density {
        CardDensity::Compact => 2.,
        CardDensity::Standard => 4.,
        CardDensity::Spacious => 6.,
    }
}

pub fn thumbnail_size(density: CardDensity) -> (f32, f32) {
    match density {
        CardDensity::Compact => (96., 56.),
        CardDensity::Standard => (120., 68.),
        CardDensity::Spacious => (140., 88.),
    }
}
pub const GROUP_BAR_HEIGHT: f32 = 42.;
pub const DRAG_MOTION_DURATION: Duration = crate::design_system::SLOW;
pub const DRAG_EDGE_ZONE: f32 = 32.;
pub const DRAG_SCROLL_INTERVAL: Duration = Duration::from_millis(60);
pub const HISTORY_DRAG_SCROLL_TICKS: usize = 2;
pub const MOTION_DURATION: Duration = crate::design_system::NORMAL;

/// Interpolate painted channels, not HSL hue: neutral-to-blue must not pass
/// through green. Premultiplication also excludes invisible endpoint colors.
#[derive(Clone, Copy, PartialEq)]
struct TransitionColor(Rgba);

impl TransitionColor {
    fn new(color: Hsla) -> Self {
        let color: Rgba = color.into();
        Self(Rgba {
            r: color.r * color.a,
            g: color.g * color.a,
            b: color.b * color.a,
            a: color.a,
        })
    }

    fn color(self) -> Hsla {
        let color = self.0;
        if color.a == 0. {
            return transparent_black();
        }
        Rgba {
            r: color.r / color.a,
            g: color.g / color.a,
            b: color.b / color.a,
            a: color.a,
        }
        .into()
    }
}

impl gpui_kit::base::motion::Interpolate for TransitionColor {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        let progress = progress.clamp(0., 1.);
        let mix = |from: f32, to: f32| from + (to - from) * progress;
        Self(Rgba {
            r: mix(self.0.r, target.0.r),
            g: mix(self.0.g, target.0.g),
            b: mix(self.0.b, target.0.b),
            a: mix(self.0.a, target.0.a),
        })
    }
}

pub fn transition_color(
    id: impl Into<ElementId>,
    target: Hsla,
    transition: gpui_kit::base::motion::Transition,
    window: &mut Window,
    cx: &mut App,
) -> Hsla {
    gpui_kit::base::motion::transition(
        id.into(),
        TransitionColor::new(target),
        transition,
        window,
        cx,
    )
    .color()
}

fn motion(cx: &App) -> Animation {
    let easing = cx.theme().motion.easing_enter.clone();
    Animation::new(MOTION_DURATION).with_easing(move |progress| easing.sample(progress))
}

pub fn reveal(element: Div, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .relative()
        .with_animation(id, motion(cx), |element, progress| {
            element.top(px(SPACE_1 * (1. - progress))).opacity(progress)
        })
        .into_any_element()
}

pub fn reflow(element: Div, offset: f32, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    let easing = cx.theme().motion.easing_move.clone();
    RowTranslation::new(element)
        .with_animation(
            id,
            Animation::new(DRAG_MOTION_DURATION)
                .with_easing(move |progress| easing.sample(progress)),
            move |mut element, progress| {
                element.from = offset * (1. - progress);
                element.to = element.from;
                element
            },
        )
        .into_any_element()
}

#[derive(Default)]
pub struct DragProgress {
    painted: Cell<f32>,
    frame: Cell<Option<f32>>,
}

impl DragProgress {
    pub fn get(&self) -> f32 {
        self.painted.get()
    }

    pub fn reset(&self) {
        self.painted.set(0.);
        self.frame.set(None);
    }

    #[cfg(test)]
    pub fn set(&self, progress: f32) {
        self.painted.set(progress);
    }
}

pub fn drag_yield(
    element: Div,
    from: f32,
    to: f32,
    started: Option<Instant>,
    progress: Rc<DragProgress>,
    cx: &App,
) -> AnyElement {
    let mut translated = RowTranslation::new(element);
    translated.from = from;
    translated.to = to;
    translated.started = started;
    translated.progress = Some(progress);
    if cx.reduce_motion() {
        translated.started = None;
    }
    translated.into_any_element()
}

// The virtual list owns the row's layout origin. Translate only its child during
// prepaint so drawing and hitboxes move together without changing Taffy geometry.
struct RowTranslation {
    child: AnyElement,
    from: f32,
    to: f32,
    started: Option<Instant>,
    progress: Option<Rc<DragProgress>>,
}

impl RowTranslation {
    fn new(child: Div) -> Self {
        Self {
            child: child.into_any_element(),
            from: 0.,
            to: 0.,
            started: None,
            progress: None,
        }
    }
}

impl IntoElement for RowTranslation {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for RowTranslation {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let sample = || {
            self.started.map_or(1., |started| {
                cx.theme().motion.easing_move.sample(
                    (started.elapsed().as_secs_f32() / DRAG_MOTION_DURATION.as_secs_f32()).min(1.),
                )
            })
        };
        let progress = if let Some(shared) = &self.progress {
            if let Some(progress) = shared.frame.get() {
                progress
            } else {
                let progress = sample();
                shared.frame.set(Some(progress));
                shared.painted.set(progress);
                let shared = shared.clone();
                window.on_next_frame(move |_, _| shared.frame.set(None));
                progress
            }
        } else {
            sample()
        };
        let offset = self.from + (self.to - self.from) * progress;
        window.with_element_offset(point(px(0.), px(offset)), |window| {
            self.child.prepaint(window, cx);
        });
        if progress < 1. && self.from != self.to {
            window.request_animation_frame();
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod color_transition_tests {
    use super::TransitionColor;
    use gpui_kit::base::motion::Interpolate as _;
    use gpui_kit::{Hsla, Rgba, rgb};

    fn assert_channels_between(color: Hsla, from: Hsla, to: Hsla) {
        let color: Rgba = color.into();
        let from: Rgba = from.into();
        let to: Rgba = to.into();
        for (value, start, end) in [
            (color.r, from.r, to.r),
            (color.g, from.g, to.g),
            (color.b, from.b, to.b),
        ] {
            assert!(value >= start.min(end) - 0.00001);
            assert!(value <= start.max(end) + 0.00001);
        }
    }

    #[test]
    fn neutral_blue_hover_never_introduces_green_on_entry_exit_or_retarget() {
        for (neutral, blue) in [(0xffffff, 0xeff6ff), (0x252525, 0x284d80)] {
            let neutral: Hsla = rgb(neutral).into();
            let blue: Hsla = rgb(blue).into();
            let start = TransitionColor::new(neutral);
            let target = TransitionColor::new(blue);
            for step in 0..=20 {
                let progress = step as f32 / 20.;
                let entering = start.interpolate(&target, progress);
                assert_channels_between(entering.color(), neutral, blue);
                let interrupted = start.interpolate(&target, 0.37);
                let leaving = interrupted.interpolate(&start, progress);
                assert_channels_between(leaving.color(), neutral, blue);
                let reentered = leaving.interpolate(&target, 0.43);
                assert_channels_between(reentered.color(), neutral, blue);
            }
        }
    }

    #[test]
    fn transparent_endpoint_rgb_cannot_tint_a_fading_surface() {
        let invisible = TransitionColor::new(
            Rgba {
                r: 1.,
                g: 0.,
                b: 0.,
                a: 0.,
            }
            .into(),
        );
        let blue = TransitionColor::new(
            Rgba {
                r: 0.,
                g: 0.,
                b: 1.,
                a: 0.6,
            }
            .into(),
        );
        for step in 1..=10 {
            let progress = step as f32 / 10.;
            let color: Rgba = invisible.interpolate(&blue, progress).color().into();
            assert!(color.r.abs() < 0.00001);
            assert!(color.g.abs() < 0.00001);
            assert!((color.b - 1.).abs() < 0.00001);
            assert!((color.a - 0.6 * progress).abs() < 0.00001);
        }
        let hidden: Rgba = blue.interpolate(&invisible, 1.).color().into();
        assert_eq!((hidden.r, hidden.g, hidden.b, hidden.a), (0., 0., 0., 0.));
    }
}
