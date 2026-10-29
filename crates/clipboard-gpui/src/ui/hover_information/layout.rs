use super::{GAP, InformationPlacement, ROW_HEIGHT};
use gpui_kit::{Bounds, Pixels};
use std::ops::Range;

pub(super) struct InformationLayout {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) align_end: bool,
    pub(super) rows: Vec<Range<usize>>,
}

#[derive(Clone, Copy)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl From<Bounds<Pixels>> for Rect {
    fn from(bounds: Bounds<Pixels>) -> Self {
        Self {
            left: bounds.origin.x.into(),
            top: bounds.origin.y.into(),
            right: bounds.right().into(),
            bottom: bounds.bottom().into(),
        }
    }
}

impl Rect {
    fn intersect(self, other: Self) -> Option<Self> {
        let intersection = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (intersection.right > intersection.left && intersection.bottom > intersection.top)
            .then_some(intersection)
    }

    fn outside(self, viewport: Self) -> [Self; 4] {
        [
            Self {
                bottom: self.top - GAP,
                ..viewport
            },
            Self {
                top: self.bottom + GAP,
                ..viewport
            },
            Self {
                left: self.right + GAP,
                ..viewport
            },
            Self {
                right: self.left - GAP,
                ..viewport
            },
        ]
    }
}

fn row_ranges(widths: &[f32], available: f32) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= widths.len() {
            return None;
        }
        let first = start;
        let mut used = widths[start].min(available);
        start += 1;
        while start < widths.len() {
            let next = used + GAP + widths[start].min(available);
            if next > available + 0.01 {
                break;
            }
            used = next;
            start += 1;
        }
        Some(first..start)
    })
}

pub(super) fn place_information(
    placement: &InformationPlacement,
    widths: &[f32],
) -> Option<InformationLayout> {
    if widths.is_empty() {
        return None;
    }
    let surface = Rect::from(placement.surface);
    let owner = Rect::from(placement.owner);
    let viewport = Rect {
        left: GAP,
        top: GAP,
        right: f32::from(placement.viewport.width) - GAP,
        bottom: f32::from(placement.viewport.height) - GAP,
    };
    let align_end = surface.right <= owner.left;
    let natural_width = widths.iter().sum::<f32>() + GAP * (widths.len() - 1) as f32;
    let mut safe_areas = owner.outside(viewport);
    let preferred = if surface.left >= owner.right {
        2
    } else if align_end {
        3
    } else if surface.top >= owner.bottom {
        1
    } else {
        0
    };
    safe_areas.swap(0, preferred);
    let mut free_areas = surface.outside(viewport);
    if align_end {
        free_areas.swap(2, 3);
    }
    // Above first, then below, then the outer side. Every candidate excludes both windows.
    for free in free_areas {
        for safe in safe_areas {
            let Some(area) = viewport
                .intersect(free)
                .and_then(|area| area.intersect(safe))
            else {
                continue;
            };
            let maximum = (area.right - area.left).min(960.0);
            if maximum < 160.0 || area.bottom - area.top < ROW_HEIGHT {
                continue;
            }
            let width = natural_width.max(160.0).min(maximum);
            let row_count = row_ranges(widths, width).count();
            let height = ROW_HEIGHT * row_count as f32 + GAP * (row_count - 1) as f32;
            if height > area.bottom - area.top {
                continue;
            }
            let left = if align_end {
                surface.right - width
            } else {
                surface.left
            };
            return Some(InformationLayout {
                left: left.clamp(area.left, area.right - width),
                top: surface.top.clamp(area.top, area.bottom - height),
                width,
                height,
                align_end,
                rows: row_ranges(widths, width).collect(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::preferences::LanguagePreference;
    use gpui_kit::{point, px, size};

    fn placement(surface: [f32; 4], owner: [f32; 4]) -> InformationPlacement {
        let bounds = |[x, y, width, height]: [f32; 4]| {
            Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
        };
        InformationPlacement {
            surface: bounds(surface),
            owner: bounds(owner),
            viewport: size(px(1000.0), px(700.0)),
            percentage: None,
            language: LanguagePreference::English,
        }
    }

    #[test]
    fn prefers_above_and_wraps_without_covering_owner() {
        let placement = placement([550.0, 250.0, 350.0, 300.0], [50.0, 50.0, 450.0, 600.0]);
        let layout = place_information(&placement, &[200.0, 200.0, 200.0]).unwrap();
        assert!(layout.top + layout.height <= 250.0 - GAP);
        assert!(layout.left >= 500.0 + GAP);
        assert_eq!(layout.rows, vec![0..2, 2..3]);
    }

    #[test]
    fn falls_below_when_above_is_too_short() {
        let placement = placement([550.0, 10.0, 350.0, 300.0], [50.0, 50.0, 450.0, 600.0]);
        let layout = place_information(&placement, &[200.0, 200.0]).unwrap();
        assert!(layout.top >= 310.0 + GAP);
        assert!(layout.left >= 500.0 + GAP);
    }

    #[test]
    fn omits_strip_instead_of_covering_preview_or_owner() {
        let placement = placement([508.0, 0.0, 492.0, 700.0], [0.0, 0.0, 500.0, 700.0]);
        assert!(place_information(&placement, &[200.0, 200.0]).is_none());
    }
}
