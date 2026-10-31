use super::{GAP, InformationPlacement, ROW_HEIGHT};
use gpui_kit::{Bounds, Pixels};

pub(super) struct InformationLayout {
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) align_end: bool,
    pub(super) rows: Vec<Vec<usize>>,
    pub(super) widths: Vec<f32>,
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

const MIN_FLEXIBLE: f32 = 140.0;

/// Place each chip on the earliest row with room. A flexible chip (a long path)
/// shrinks into the largest leftover instead of forcing every later chip onto a new row.
pub(super) fn pack_rows(
    widths: &[f32],
    flexible: &[bool],
    available: f32,
) -> (Vec<Vec<usize>>, Vec<f32>) {
    let mut placed = widths.to_vec();
    let mut rows: Vec<(f32, Vec<usize>)> = Vec::new();
    for (index, preferred) in widths.iter().copied().enumerate() {
        let width = preferred.min(available).max(1.0);
        if let Some(row) = rows.iter_mut().find(|(used, chips)| {
            let next = if chips.is_empty() {
                width
            } else {
                *used + GAP + width
            };
            next <= available + 0.01
        }) {
            row.0 = if row.1.is_empty() {
                width
            } else {
                row.0 + GAP + width
            };
            row.1.push(index);
            placed[index] = width;
            continue;
        }
        let flexible = flexible.get(index).copied().unwrap_or(false);
        let gap = rows
            .iter()
            .enumerate()
            .filter_map(|(row_index, (used, chips))| {
                let gap = available - *used - if chips.is_empty() { 0.0 } else { GAP };
                (flexible && gap >= MIN_FLEXIBLE).then_some((gap, row_index))
            });
        if let Some((gap, row_index)) = gap.max_by(|left, right| left.0.total_cmp(&right.0)) {
            let width = gap.min(width);
            placed[index] = width;
            rows[row_index].0 += GAP + width;
            rows[row_index].1.push(index);
            continue;
        }
        placed[index] = width;
        rows.push((width, vec![index]));
    }
    (rows.into_iter().map(|(_, chips)| chips).collect(), placed)
}

pub(super) fn place_information(
    placement: &InformationPlacement,
    widths: &[f32],
    flexible: &[bool],
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
            let (rows, placed) = pack_rows(widths, flexible, width);
            let row_count = rows.len();
            let height = ROW_HEIGHT * row_count as f32 + GAP * (row_count.saturating_sub(1)) as f32;
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
                rows,
                widths: placed,
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
        let layout = place_information(&placement, &[200.0, 200.0, 200.0], &[false; 3]).unwrap();
        assert!(layout.top + layout.height <= 250.0 - GAP);
        assert!(layout.left >= 500.0 + GAP);
        assert_eq!(layout.rows, vec![vec![0, 1], vec![2]]);
    }

    #[test]
    fn falls_below_when_above_is_too_short() {
        let placement = placement([550.0, 10.0, 350.0, 300.0], [50.0, 50.0, 450.0, 600.0]);
        let layout = place_information(&placement, &[200.0, 200.0], &[false; 2]).unwrap();
        assert!(layout.top >= 310.0 + GAP);
        assert!(layout.left >= 500.0 + GAP);
    }

    #[test]
    fn omits_strip_instead_of_covering_preview_or_owner() {
        let placement = placement([508.0, 0.0, 492.0, 700.0], [0.0, 0.0, 500.0, 700.0]);
        assert!(place_information(&placement, &[200.0, 200.0], &[false; 2]).is_none());
    }

    #[test]
    fn short_chip_fills_an_earlier_gap_instead_of_a_new_row() {
        let (rows, _) = pack_rows(&[100.0, 220.0, 80.0], &[false, true, false], 200.0);
        assert_eq!(rows, vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn long_path_shrinks_into_the_open_gap() {
        let (rows, placed) = pack_rows(&[100.0, 300.0], &[false, true], 250.0);
        assert_eq!(rows, vec![vec![0, 1]]);
        assert!((placed[1] - 142.0).abs() < 0.1);
    }
}
