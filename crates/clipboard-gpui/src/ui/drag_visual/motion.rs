use super::*;

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
    pub(in crate::ui) fn hover_target(
        &mut self,
        source: usize,
        items: impl Iterator<Item = (i64, bool)> + Clone,
        offsets: &[Pixels],
        pointer_y: Pixels,
        previous: Option<DropTarget>,
    ) -> Option<DropTarget> {
        offsets.get(source + 1)?;
        let (_, pinned) = items.clone().nth(source)?;
        let progress = self.started.map_or(1., |_| self.progress.get());
        let top_at = |index| {
            let (from, to) = self.offsets(index, offsets);
            offsets[index] + px(from + (to - from) * progress)
        };
        let source_top = top_at(source);
        let mut hovered = None;
        for (index, (id, is_pinned)) in items.enumerate().take(offsets.len() - 1) {
            // The source is invisible; its moving slot must not occlude a real card.
            if index == source {
                continue;
            }
            let top = top_at(index);
            let bottom = top + offsets[index + 1] - offsets[index];
            if pointer_y >= top && pointer_y < bottom {
                if self.hovered == Some(id) {
                    return previous;
                }
                hovered = Some((id, is_pinned, top));
                break;
            }
        }
        let Some((id, is_pinned, top)) = hovered else {
            self.hovered = None;
            return previous;
        };
        if is_pinned != pinned {
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
        items: &[DbClipboardItem],
        row_offsets: &[Pixels],
    ) {
        if self.request == request {
            return;
        }
        let next = request.and_then(|(source, target)| {
            let source = items.iter().position(|item| item.id == source)?;
            let target_index = items.iter().position(|item| item.id == target.id)?;
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
        self.from_offsets
            .retain(|index, offset| *index + 1 < row_offsets.len() && *offset != 0.);
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
