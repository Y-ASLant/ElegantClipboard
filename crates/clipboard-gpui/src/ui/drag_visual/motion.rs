use super::*;

fn candidate_rows(offsets: &[Pixels], pointer_y: Pixels, radius: Pixels) -> std::ops::Range<usize> {
    let first = offsets[1..].partition_point(|bottom| *bottom <= pointer_y - radius);
    let end = offsets[..offsets.len() - 1].partition_point(|top| *top <= pointer_y + radius);
    first..end
}

impl DragPlacement {
    pub(in crate::ui) fn new(source: usize, target: usize, after: bool) -> Option<Self> {
        let insertion = target + usize::from(after);
        let insertion = insertion - usize::from(source < insertion);
        (source != insertion).then_some(Self { source, insertion })
    }

    pub(in crate::ui) fn shift(self, index: usize, offsets: &[Pixels]) -> f32 {
        let source_height = f32::from(offsets[self.source + 1] - offsets[self.source]);
        if index == self.source {
            if self.insertion > self.source {
                f32::from(offsets[self.insertion + 1] - offsets[self.source + 1])
            } else {
                f32::from(offsets[self.insertion] - offsets[self.source])
            }
        } else if (self.source + 1..=self.insertion).contains(&index) {
            -source_height
        } else if (self.insertion..self.source).contains(&index) {
            source_height
        } else {
            0.
        }
    }
}

impl DragMotionState {
    fn hover_candidates(
        &self,
        source: usize,
        offsets: &[Pixels],
        pointer_y: Pixels,
    ) -> std::ops::Range<usize> {
        // Geometry stays fixed throughout a drag. Non-source placement shifts
        // are at most the source height, and interruption blends those shifts.
        // Retained bounds also cover a previous drag's now-visible source.
        let from_bound = self
            .from_offset_bounds
            .iter()
            .filter(|(index, _)| *index != source)
            .map(|(_, offset)| *offset)
            .fold(0_f32, f32::max);
        let to_bound = self
            .to
            .filter(|to| to.source + 1 < offsets.len() && to.insertion + 1 < offsets.len())
            .map_or(0., |to| {
                let height = f32::from(offsets[to.source + 1] - offsets[to.source]);
                if to.source == source {
                    height
                } else {
                    height.max(to.shift(to.source, offsets).abs())
                }
            });
        candidate_rows(offsets, pointer_y, px(from_bound.max(to_bound)))
    }

    pub(in crate::ui) fn hover_target<G: Copy + PartialEq>(
        &mut self,
        source: usize,
        item_at: impl Fn(usize) -> Option<(i64, G)>,
        offsets: &[Pixels],
        pointer_y: Pixels,
        previous: Option<DropTarget>,
    ) -> Option<DropTarget> {
        offsets.get(source + 1)?;
        let (_, group) = item_at(source)?;
        let progress = self.started.map_or(1., |_| self.progress.get());
        let top_at = |index| {
            let (from, to) = self.offsets(index, offsets);
            offsets[index] + px(from + (to - from) * progress)
        };
        let source_top = top_at(source);
        let candidates = self.hover_candidates(source, offsets, pointer_y);
        let mut hovered = None;
        // Ascending model order preserves first-row-wins during animated overlap.
        for index in candidates {
            // The source is invisible; its moving slot must not occlude a real card.
            if index == source {
                continue;
            }
            let top = top_at(index);
            let bottom = top + offsets[index + 1] - offsets[index];
            if pointer_y >= top && pointer_y < bottom {
                let Some((id, item_group)) = item_at(index) else {
                    break;
                };
                if self.hovered == Some(id) {
                    return previous;
                }
                hovered = Some((id, item_group, top));
                break;
            }
        }
        let Some((id, item_group, top)) = hovered else {
            self.hovered = None;
            return previous;
        };
        if item_group != group {
            self.hovered = None;
            return previous;
        }
        self.hovered = Some(id);
        Some(DropTarget {
            id,
            after: top > source_top,
        })
    }

    pub(in crate::ui) fn update(
        &mut self,
        request: Option<(i64, DropTarget)>,
        index_of: impl Fn(i64) -> Option<usize>,
        row_offsets: &[Pixels],
    ) {
        if self.request == request {
            return;
        }
        let next = request.and_then(|(source, target)| {
            let source = index_of(source)?;
            let target_index = index_of(target.id)?;
            DragPlacement::new(source, target_index, target.after)
        });
        self.transition(next, row_offsets, std::time::Instant::now());
        self.request = request;
    }

    pub(super) fn transition(
        &mut self,
        next: Option<DragPlacement>,
        row_offsets: &[Pixels],
        now: std::time::Instant,
    ) {
        if self.to == next {
            return;
        }
        let progress = self.started.map_or(1., |_| self.progress.get());
        for offset in self.from_offsets.values_mut() {
            *offset *= 1. - progress;
        }
        if let Some(previous) = self
            .to
            .filter(|to| to.source + 1 < row_offsets.len() && to.insertion + 1 < row_offsets.len())
        {
            for index in
                previous.source.min(previous.insertion)..=previous.source.max(previous.insertion)
            {
                *self.from_offsets.entry(index).or_default() +=
                    previous.shift(index, row_offsets) * progress;
            }
        }
        // Excluding one invisible source needs the two largest magnitudes to
        // retain the maximum bound for every remaining visible row.
        self.from_offset_bounds = [(0, 0.); 2];
        self.from_offsets.retain(|index, offset| {
            if *index + 1 >= row_offsets.len() || *offset == 0. {
                return false;
            }
            let entry = (*index, offset.abs());
            if entry.1 > self.from_offset_bounds[0].1 {
                self.from_offset_bounds[1] = self.from_offset_bounds[0];
                self.from_offset_bounds[0] = entry;
            } else if entry.1 > self.from_offset_bounds[1].1 {
                self.from_offset_bounds[1] = entry;
            }
            true
        });
        self.to = next;
        self.started = Some(now);
        self.progress.reset();
    }

    pub(in crate::ui) fn offsets(&self, index: usize, row_offsets: &[Pixels]) -> (f32, f32) {
        (
            self.from_offsets.get(&index).copied().unwrap_or(0.),
            self.to
                .filter(|to| {
                    to.source + 1 < row_offsets.len() && to.insertion + 1 < row_offsets.len()
                })
                .map_or(0., |to| to.shift(index, row_offsets)),
        )
    }
}

#[cfg(test)]
mod tests;
