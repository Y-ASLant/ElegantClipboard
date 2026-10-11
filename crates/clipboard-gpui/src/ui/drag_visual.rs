use super::*;
impl DragPlacement {
    pub(super) fn new(source: usize, target: usize, after: bool) -> Option<Self> {
        let insertion = target + usize::from(after);
        let insertion = insertion - usize::from(source < insertion);
        (source != insertion).then_some(Self { source, insertion })
    }

    pub(super) fn shift(self, index: usize, offsets: &[Pixels]) -> f32 {
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
    pub(super) fn update(
        &mut self,
        request: Option<(i64, DropTarget)>,
        items: &[clipboard_core::database::ClipboardItem],
    ) {
        if self.request == request {
            return;
        }
        self.from = self.to;
        self.to = request.and_then(|(source, target)| {
            let source = items.iter().position(|item| item.id == source)?;
            let target_index = items.iter().position(|item| item.id == target.id)?;
            DragPlacement::new(source, target_index, target.after)
        });
        self.request = request;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn offsets(&self, index: usize, row_offsets: &[Pixels]) -> (f32, f32) {
        (
            self.from.map_or(0., |from| from.shift(index, row_offsets)),
            self.to.map_or(0., |to| to.shift(index, row_offsets)),
        )
    }
}
impl Render for HistoryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        visual::reveal(
            div()
                .w(px(280.))
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary)
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .shadow_md()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(Icon::new(gpui_kit::assets::IconName::GripVertical).xsmall())
                        .child(format!(
                            "{}{}",
                            if self.pinned {
                                tr(self.language, "置顶 · ", "Pinned · ")
                            } else {
                                ""
                            },
                            self.kind
                        )),
                )
                .child(
                    div()
                        .rounded_sm()
                        .bg(cx.theme().muted)
                        .px_2()
                        .py_1()
                        .text_sm()
                        .line_clamp(2)
                        .text_ellipsis()
                        .child(self.preview.clone()),
                ),
            ("drag-preview", self.id as usize),
            cx,
        )
    }
}
