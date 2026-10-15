use super::*;

#[derive(Clone)]
pub(super) struct HistoryCardSnapshot {
    pub(super) item: DbClipboardItem,
    pub(super) display: DisplayPreference,
    pub(super) language: LanguagePreference,
    pub(super) summary: String,
    pub(super) preview_text: String,
    pub(super) highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    pub(super) thumbnail_path: Option<PathBuf>,
    pub(super) file_warning: bool,
    pub(super) file_icon: IconName,
    pub(super) selected: bool,
    pub(super) batch_mode: bool,
}

impl HistoryCardSnapshot {
    pub(super) fn surface(&self, preview: bool, cx: &App) -> Div {
        let is_image = self.item.content_type == "image";
        let show_file_icon = self.item.content_type == "files" && self.thumbnail_path.is_none();
        let spacing = visual::card_spacing(self.display.card_density);
        let (thumbnail_width, thumbnail_height) = if is_image {
            visual::thumbnail_size(self.display.card_density)
        } else {
            (48., 36.)
        };
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let source_icon = self
            .item
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) =
            source_app_parts(self.display.source_app_display, source_icon.is_some());
        div()
            .h_full()
            .group("")
            .when(self.batch_mode, |card| card.px_3())
            .when(!self.batch_mode, |card| card.px(px(22.)))
            .py(px(spacing))
            .rounded_md()
            .border_1()
            .when(preview, |card| card.rounded(px(8.)).border(px(1.5)))
            .border_color(if self.selected {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .bg(if preview {
                cx.theme().background.opacity(0.8)
            } else if self.selected {
                cx.theme().accent
            } else {
                cx.theme().background
            })
            .relative()
            .flex()
            .flex_col()
            .gap(px(spacing))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded_sm()
                    .when(is_image && !preview, |body| body.bg(cx.theme().muted))
                    .px_2()
                    .py(px(spacing))
                    .flex()
                    .when(is_image, |body| body.justify_center())
                    .gap_3()
                    .when(show_file_icon, |body| {
                        body.child(
                            div()
                                .w(px(thumbnail_width))
                                .h(px(thumbnail_height))
                                .flex_none()
                                .rounded_sm()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(if self.file_warning {
                                    cx.theme().danger.opacity(0.12)
                                } else {
                                    cx.theme().primary.opacity(0.12)
                                })
                                .child(Icon::new(self.file_icon.clone()).large().text_color(
                                    if self.file_warning {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().primary
                                    },
                                )),
                        )
                    })
                    .when(is_image && self.thumbnail_path.is_none(), |body| {
                        body.child(image_unavailable)
                    })
                    .when_some(self.thumbnail_path.clone(), |body, path| {
                        body.child(
                            div()
                                .w(px(thumbnail_width))
                                .h(px(thumbnail_height))
                                .flex_none()
                                .rounded_sm()
                                .overflow_hidden()
                                .child(
                                    img(path)
                                        .size_full()
                                        .object_fit(ObjectFit::Contain)
                                        .with_fallback(move || {
                                            div().child(image_unavailable).into_any_element()
                                        }),
                                ),
                        )
                    })
                    .when(!is_image, |body| {
                        body.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(if self.file_warning {
                                    cx.theme().danger
                                } else {
                                    cx.theme().foreground
                                })
                                .line_height(px(20.))
                                .line_clamp(self.display.card_max_lines as usize)
                                .text_ellipsis()
                                .child(
                                    StyledText::new(self.preview_text.clone())
                                        .with_highlights(self.highlights.clone()),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(24.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .text_xs()
                    .text_color(if self.file_warning {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.summary.clone()),
                    )
                    .when(self.display.show_source_app, |footer| {
                        footer.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .max_w(px(118.))
                                .overflow_hidden()
                                .when(!preview, |source| {
                                    source.group_hover("", |source| source.invisible())
                                })
                                .when_some(
                                    show_source_icon.then_some(source_icon).flatten(),
                                    |source, path| {
                                        source.child(
                                            img(PathBuf::from(path))
                                                .w(px(16.))
                                                .h(px(16.))
                                                .flex_none()
                                                .object_fit(ObjectFit::Contain)
                                                .with_fallback(|| div().into_any_element()),
                                        )
                                    },
                                )
                                .when(show_source_name, |source| {
                                    source.child(
                                        div().min_w_0().overflow_hidden().text_ellipsis().child(
                                            self.item.source_app_name.clone().unwrap_or_default(),
                                        ),
                                    )
                                }),
                        )
                    }),
            )
    }
}

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
    pub(super) fn hover_target(
        &mut self,
        source: usize,
        items: impl Iterator<Item = (i64, bool)> + Clone,
        offsets: &[Pixels],
        pointer_y: Pixels,
        previous: Option<DropTarget>,
    ) -> Option<DropTarget> {
        let (_, pinned) = items.clone().nth(source)?;
        let shift = |index| {
            self.to
                .map_or(0., |placement| placement.shift(index, offsets))
        };
        let source_top = offsets[source] + px(shift(source));
        let hovered = items.enumerate().find_map(|(index, (id, is_pinned))| {
            let top = offsets[index] + px(shift(index));
            let bottom = offsets[index + 1] + px(shift(index));
            (pointer_y >= top && pointer_y < bottom).then_some((index, id, is_pinned, top))
        });
        let Some((index, id, is_pinned, top)) = hovered else {
            self.hovered = None;
            return previous;
        };
        if index == source || is_pinned != pinned {
            self.hovered = None;
            return previous;
        }
        if self.hovered == Some(id) {
            return previous;
        }
        self.hovered = Some(id);
        Some(DropTarget {
            id,
            after: top > source_top,
            allowed: true,
        })
    }

    pub(super) fn update(
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

    fn transition(
        &mut self,
        next: Option<DragPlacement>,
        row_offsets: &[Pixels],
        now: std::time::Instant,
    ) {
        let progress = self.started.map_or(1., |started| {
            ease_in_out(
                (now.saturating_duration_since(started).as_secs_f32()
                    / visual::DRAG_MOTION_DURATION.as_secs_f32())
                .min(1.),
            )
        });
        self.from_offsets = (0..row_offsets.len().saturating_sub(1))
            .map(|index| {
                let (from, to) = self.offsets(index, row_offsets);
                from + (to - from) * progress
            })
            .collect();
        self.to = next;
        self.started = Some(now);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn offsets(&self, index: usize, row_offsets: &[Pixels]) -> (f32, f32) {
        (
            self.from_offsets.get(index).copied().unwrap_or(0.),
            self.to
                .filter(|to| {
                    to.source + 1 < row_offsets.len() && to.insertion + 1 < row_offsets.len()
                })
                .map_or(0., |to| to.shift(index, row_offsets)),
        )
    }
}

impl HistoryDrag {
    pub(super) fn size(&self) -> gpui_kit::Size<Pixels> {
        self.size.get()
    }

    pub(super) fn render_card(&self, cx: &App) -> AnyElement {
        div()
            .font_family("Microsoft YaHei UI")
            .w(self.size().width)
            .h(self.size().height)
            .px(px(PAGE_PADDING))
            .py(px(visual::card_spacing(self.card.display.card_density)))
            .text_color(cx.theme().foreground)
            .child(self.card.surface(true, cx))
            .into_any_element()
    }
}

impl Render for HistoryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.native_preview {
            div().into_any_element()
        } else {
            self.render_card(cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DragMotionState, DragPlacement, px, visual};

    #[test]
    fn mixed_height_hover_waits_for_leave_before_reversing() {
        for (source, first_y, leave_y, return_y, expected_after) in
            [(0, 110., 180., 50., true), (1, 110., 20., 180., false)]
        {
            let offsets = if source == 0 {
                [px(0.), px(96.), px(220.)]
            } else {
                [px(0.), px(124.), px(220.)]
            };
            let items = [(10, false), (20, false)];
            let mut motion = DragMotionState::default();
            let target = motion
                .hover_target(source, items.into_iter(), &offsets, px(first_y), None)
                .unwrap();
            assert_eq!(target.after, expected_after);
            motion.to = DragPlacement::new(source, 1 - source, target.after);
            let repeated = motion.hover_target(
                source,
                items.into_iter(),
                &offsets,
                px(first_y + 1.),
                Some(target),
            );
            assert_eq!(
                repeated.map(|target| (target.id, target.after)),
                Some((target.id, target.after))
            );
            motion.hover_target(
                source,
                items.into_iter(),
                &offsets,
                px(leave_y),
                Some(target),
            );
            let reverse = motion
                .hover_target(
                    source,
                    items.into_iter(),
                    &offsets,
                    px(return_y),
                    Some(target),
                )
                .unwrap();
            assert_eq!(reverse.after, !expected_after);
            assert!(DragPlacement::new(source, 1 - source, reverse.after).is_none());
        }
    }

    #[test]
    fn hovering_another_pin_partition_preserves_the_valid_placement() {
        let offsets = [px(0.), px(96.), px(192.), px(316.)];
        let mut motion = DragMotionState::default();
        let items = [(10, true), (20, false), (30, false)];
        let target = motion
            .hover_target(1, items.into_iter(), &offsets, px(220.), None)
            .unwrap();
        motion.to = DragPlacement::new(1, 2, target.after);
        let blocked = motion
            .hover_target(1, items.into_iter(), &offsets, px(20.), Some(target))
            .unwrap();
        assert_eq!((blocked.id, blocked.after), (target.id, target.after));
        assert_eq!(motion.hovered, None);
    }

    #[test]
    fn reversing_a_drag_continues_from_the_visible_offset() {
        let offsets = [px(0.), px(96.), px(220.), px(308.)];
        let start = std::time::Instant::now();
        let mut motion = DragMotionState::default();
        motion.transition(DragPlacement::new(0, 2, true), &offsets, start);
        motion.transition(None, &offsets, start + visual::DRAG_MOTION_DURATION / 2);
        assert_eq!(motion.offsets(1, &offsets), (-48., 0.));
        assert_eq!(motion.offsets(0, &offsets), (106., 0.));
        motion.transition(
            DragPlacement::new(0, 1, true),
            &offsets,
            start + visual::DRAG_MOTION_DURATION,
        );
        assert_eq!(motion.offsets(1, &offsets), (-24., -96.));
    }
}
