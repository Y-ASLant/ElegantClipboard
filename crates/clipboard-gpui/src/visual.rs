//! Shared spacing, control sizing and motion for the native UI.
use clipboard_core::preferences::CardDensity;
use gpui_kit::*;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
pub const PAGE_PADDING: f32 = 8.;
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
pub const DRAG_MOTION_DURATION: Duration = Duration::from_millis(260);
pub const DRAG_EDGE_ZONE: f32 = 32.;
pub const DRAG_SCROLL_INTERVAL: Duration = Duration::from_millis(60);
pub const HISTORY_DRAG_SCROLL_TICKS: usize = 2;
pub const MOTION_DURATION: Duration = Duration::from_millis(180);

fn motion() -> Animation {
    Animation::new(MOTION_DURATION).with_easing(ease_out_quint())
}

pub fn reveal(element: Div, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    element
        .with_animation(id, motion(), |element, progress| {
            element.opacity(0.65 + 0.35 * progress)
        })
        .into_any_element()
}

pub fn reflow(element: Div, offset: f32, id: impl Into<ElementId>, cx: &App) -> AnyElement {
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    RowTranslation::new(element)
        .with_animation(
            id,
            Animation::new(DRAG_MOTION_DURATION).with_easing(ease_in_out),
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
                ease_in_out(
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
