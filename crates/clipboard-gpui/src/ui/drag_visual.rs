mod motion;

use super::*;

#[derive(Clone)]
pub(super) struct HistoryCardSnapshot {
    pub(super) is_image: bool,
    pub(super) is_files: bool,
    pub(super) source_app_name: Option<String>,
    pub(super) source_app_icon: Option<String>,
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
        let is_image = self.is_image;
        let show_file_icon = self.is_files && self.thumbnail_path.is_none();
        let spacing = visual::card_spacing(self.display.card_density);
        let (thumbnail_width, thumbnail_height) = if is_image {
            visual::thumbnail_size(self.display.card_density)
        } else {
            (48., 36.)
        };
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let source_icon = self
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
                                            self.source_app_name.clone().unwrap_or_default(),
                                        ),
                                    )
                                }),
                        )
                    }),
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
    fn hover_uses_painted_progress_and_ignores_the_invisible_source() {
        let offsets = [px(0.), px(100.), px(200.), px(300.)];
        let items = [(10, false), (20, false), (30, false)];
        let mut motion = DragMotionState::default();
        motion.transition(
            DragPlacement::new(0, 2, true),
            &offsets,
            std::time::Instant::now(),
        );
        motion.progress.set(0.25);
        // Row 30 is still at 175..275, while its final position is 100..200.
        // The hidden source occupies 50..150 and must not block row 20.
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(225.), None)
                .unwrap()
                .id,
            30
        );
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(125.), None)
                .unwrap()
                .id,
            20
        );
    }

    #[test]
    fn hover_boundaries_follow_the_last_painted_frame() {
        let offsets = [px(0.), px(100.), px(200.), px(300.)];
        let items = [(10, false), (20, false), (30, false)];
        let mut motion = DragMotionState::default();
        motion.transition(
            DragPlacement::new(0, 2, true),
            &offsets,
            std::time::Instant::now(),
        );
        // A transition has been requested but no frame has moved the cards yet.
        assert!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(99.), None)
                .is_none()
        );
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(100.), None)
                .unwrap()
                .id,
            20
        );
        motion.hovered = None;
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(200.), None)
                .unwrap()
                .id,
            30
        );
        // After a final frame (including reduced motion), use final half-open bounds.
        motion.progress.set(1.);
        motion.hovered = None;
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(0.), None)
                .unwrap()
                .id,
            20
        );
        assert_eq!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(100.), None)
                .unwrap()
                .id,
            30
        );
        assert!(
            motion
                .hover_target(0, items.into_iter(), &offsets, px(200.), None)
                .is_none()
        );
    }

    #[test]
    fn a_stationary_pointer_does_not_reverse_the_same_moving_target() {
        let offsets = [px(0.), px(96.), px(220.)];
        let items = [(10, false), (20, false)];
        let mut motion = DragMotionState::default();
        let target = motion
            .hover_target(0, items.into_iter(), &offsets, px(110.), None)
            .unwrap();
        motion.transition(
            DragPlacement::new(0, 1, true),
            &offsets,
            std::time::Instant::now(),
        );
        for progress in [0., 0.25, 0.5, 0.75, 1.] {
            motion.progress.set(progress);
            let current = motion
                .hover_target(0, items.into_iter(), &offsets, px(110.), Some(target))
                .unwrap();
            assert_eq!((current.id, current.after), (target.id, target.after));
        }
    }

    #[test]
    fn reversing_a_drag_continues_from_the_visible_offset() {
        let offsets = [px(0.), px(96.), px(220.), px(308.)];
        let start = std::time::Instant::now();
        let mut motion = DragMotionState::default();
        motion.transition(DragPlacement::new(0, 2, true), &offsets, start);
        motion.progress.set(0.5);
        motion.transition(None, &offsets, start + visual::DRAG_MOTION_DURATION / 2);
        assert_eq!(motion.offsets(1, &offsets), (-48., 0.));
        assert_eq!(motion.offsets(0, &offsets), (106., 0.));
        motion.progress.set(0.5);
        motion.transition(
            DragPlacement::new(0, 1, true),
            &offsets,
            start + visual::DRAG_MOTION_DURATION,
        );
        assert_eq!(motion.offsets(1, &offsets), (-24., -96.));
    }
}
