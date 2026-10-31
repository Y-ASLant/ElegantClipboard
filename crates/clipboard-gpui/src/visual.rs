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
pub const PREVIEW_LINE_HEIGHT: f32 = 20.;
pub const METADATA_ROW_HEIGHT: f32 = 20.;
pub const FILE_THUMBNAIL_WIDTH: f32 = 48.;
pub const FILE_THUMBNAIL_HEIGHT: f32 = 36.;
/// Narrow columns across a text card. 14px UI text fits about 44 ASCII
/// characters, or 22 full-width characters, before it wraps.
pub const TEXT_COLUMNS_PER_LINE: usize = 44;
/// The file title sits beside the thumbnail, so it wraps sooner.
pub const FILE_COLUMNS_PER_LINE: usize = 36;

/// Preview lines are a cap. Narrow characters must not be counted as full-width,
/// or a two-line key reserves a blank third line.
pub fn capped_preview_lines(text: &str, max_lines: u8, columns_per_line: usize) -> u8 {
    let max_lines = usize::from(max_lines.max(1));
    let per_line = columns_per_line.max(1);
    let hard = text.split('\n').count().max(1);
    let columns = text.chars().map(column_width).sum::<usize>();
    let soft = columns.div_ceil(per_line).max(1);
    hard.max(soft).min(max_lines) as u8
}

fn column_width(ch: char) -> usize {
    match ch {
        '\n' | '\r' => 0,
        ch if is_full_width(ch) => 2,
        _ => 1,
    }
}

fn is_full_width(ch: char) -> bool {
    matches!(
        ch,
        '\u{1100}'..='\u{115F}'
            | '\u{2E80}'..='\u{A4CF}'
            | '\u{AC00}'..='\u{D7A3}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FE10}'..='\u{FE6F}'
            | '\u{FF01}'..='\u{FF60}'
            | '\u{FFE0}'..='\u{FFE6}'
            | '\u{1F300}'..='\u{1FAFF}'
    )
}

fn card_chrome(density: CardDensity) -> f32 {
    let spacing = card_spacing(density);
    // Row padding, card padding and border, the gap above metadata, then the metadata row.
    spacing * 2. + card_padding(density) * 2. + BORDER_WIDTH * 2. + spacing + METADATA_ROW_HEIGHT
}

pub fn row_height(density: CardDensity, preview_lines: u8) -> f32 {
    card_chrome(density) + PREVIEW_LINE_HEIGHT * f32::from(preview_lines.max(1))
}

pub fn media_row_height(density: CardDensity, image: bool, title_lines: u8) -> f32 {
    let thumb = if image {
        thumbnail_size(density).1
    } else {
        FILE_THUMBNAIL_HEIGHT
    };
    let title = PREVIEW_LINE_HEIGHT * f32::from(title_lines.max(1));
    card_chrome(density) + thumb.max(title)
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

#[cfg(test)]
mod card_height_tests {
    use super::{
        CardDensity, TEXT_COLUMNS_PER_LINE, capped_preview_lines, media_row_height, row_height,
    };

    #[test]
    fn unused_preview_lines_do_not_stretch_the_card() {
        let density = CardDensity::Standard;
        assert!(row_height(density, 1) < row_height(density, 2));
        assert!(row_height(density, 2) < row_height(density, 3));
        assert_eq!(capped_preview_lines("只有一行", 3, 24), 1);
        assert_eq!(capped_preview_lines("第一行\n第二行", 3, 24), 2);
        assert_eq!(capped_preview_lines("一\n二\n三\n四", 3, 24), 3);
        let narrow = "n".repeat(62);
        assert_eq!(capped_preview_lines(&narrow, 3, TEXT_COLUMNS_PER_LINE), 2);
        assert_eq!(
            capped_preview_lines(&"字".repeat(23), 3, TEXT_COLUMNS_PER_LINE),
            2
        );
        assert!(media_row_height(density, false, 1) < row_height(density, 3));
    }
}
