mod motion;

use super::presentation::UI_FONT_FAMILY;
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
    // Cloning a drag snapshot also retains the decoded pixels after list eviction.
    pub(super) thumbnail: Option<gpui_kit::ImageSource>,
    pub(super) file_warning: bool,
    pub(super) file_icon: IconName,
    pub(super) selected: bool,
}

impl HistoryCardSnapshot {
    fn metadata_row(&self, cx: &App) -> Div {
        let source_icon = self
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) =
            source_app_parts(self.display.source_app_display, source_icon.is_some());
        div()
            .h(px(visual::METADATA_ROW_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(visual::SPACE_2))
            .text_size(px(visual::TEXT_SMALL))
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
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.summary.clone()),
            )
            .when(self.display.show_source_app, |footer| {
                footer.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(visual::SPACE_1))
                        .max_w(px(118.))
                        .overflow_hidden()
                        .when_some(
                            show_source_icon.then_some(source_icon).flatten(),
                            |source, path| {
                                source.child(
                                    img(PathBuf::from(path))
                                        .w(px(visual::ICON_SIZE))
                                        .h(px(visual::ICON_SIZE))
                                        .flex_none()
                                        .object_fit(ObjectFit::Contain)
                                        .with_fallback(|| div().into_any_element()),
                                )
                            },
                        )
                        .when(show_source_name, |source| {
                            source.child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(self.source_app_name.clone().unwrap_or_default()),
                            )
                        }),
                )
            })
    }

    fn media_visual(&self, preview: bool, cx: &App) -> Div {
        let is_image = self.is_image;
        let show_file_icon = self.is_files && self.thumbnail_path.is_none();
        let (thumbnail_width, thumbnail_height) = if is_image {
            visual::thumbnail_size(self.display.card_density)
        } else {
            (visual::FILE_THUMBNAIL_WIDTH, visual::FILE_THUMBNAIL_HEIGHT)
        };
        let visual_height = thumbnail_height;
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let image_unavailable_color = cx.theme().muted_foreground;
        let unavailable = move || {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(visual::TEXT_SMALL))
                .text_color(image_unavailable_color)
                .child(image_unavailable)
        };
        let icon_color = if self.file_warning {
            cx.theme().danger
        } else {
            cx.theme().primary
        };
        div()
            .w(px(thumbnail_width))
            .h(px(visual_height))
            .flex_none()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(if show_file_icon {
                icon_color.opacity(0.12)
            } else {
                cx.theme().muted
            })
            .when(show_file_icon, |visual| {
                visual.child(
                    Icon::new(self.file_icon.clone())
                        .size(px(visual::SPACE_6))
                        .text_color(icon_color),
                )
            })
            .when(is_image && self.thumbnail_path.is_none(), |visual| {
                visual.child(unavailable())
            })
            .when_some(self.thumbnail_path.as_ref(), |visual, path| {
                let source = self.thumbnail.clone().unwrap_or_else(|| {
                    if preview {
                        // A drag started before decoding must not freeze a loading image.
                        gpui_kit::ImageSource::from(path.clone())
                    } else {
                        gpui_kit::ImageSource::Custom(std::sync::Arc::new(|_, _| None))
                    }
                });
                visual.child(
                    img(source)
                        .size_full()
                        // GPUI otherwise uses the source ratio for block layout,
                        // enlarging portrait bounds before Contain is applied.
                        .aspect_ratio(thumbnail_width / visual_height)
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(move || unavailable().into_any_element()),
                )
            })
    }

    fn content_text(&self, cx: &App) -> Div {
        let text = div()
            .min_w_0()
            .text_size(px(visual::TEXT_BODY))
            .line_height(px(visual::PREVIEW_LINE_HEIGHT))
            .line_clamp(self.display.card_max_lines as usize)
            .text_ellipsis()
            .text_color(if self.file_warning {
                cx.theme().danger
            } else {
                cx.theme().foreground
            });
        if self.is_image {
            text.child(tr(self.language, "图片", "Image"))
        } else {
            text.child(
                StyledText::new(self.preview_text.clone()).with_highlights(self.highlights.clone()),
            )
        }
    }

    pub(super) fn surface(&self, preview: bool, cx: &App) -> Div {
        let media_layout = self.is_image || self.is_files;
        let spacing = visual::card_spacing(self.display.card_density);
        let padding = visual::card_padding(self.display.card_density);
        let body = div().min_w_0().overflow_hidden().flex().items_start();
        // The summary stays on its own row. Beside a thumbnail it wraps mid-token
        // ("今天 20" / "24 · 文件") once the source name takes the remaining width.
        let body = if media_layout {
            body.gap(px(visual::SPACE_3))
                .child(self.media_visual(preview, cx))
                .child(
                    self.content_text(cx)
                        .flex_1()
                        .overflow_hidden()
                        .font_medium(),
                )
        } else {
            body.flex_1()
                .min_h_0()
                .child(self.content_text(cx).flex_1())
        };
        div()
            .h_full()
            .group("")
            .p(px(padding))
            .rounded(cx.theme().radius_lg)
            .border(px(visual::BORDER_WIDTH))
            .border_color(if self.selected {
                cx.theme().primary
            } else {
                crate::design_system::container_border(cx)
            })
            .bg(if preview {
                cx.theme().popover
            } else if self.selected {
                cx.theme().list_active
            } else {
                cx.theme().colors.list
            })
            .relative()
            .flex()
            .flex_col()
            .gap(px(spacing))
            .child(body)
            .child(self.metadata_row(cx))
    }
}

impl HistoryDrag {
    pub(super) fn size(&self) -> gpui_kit::Size<Pixels> {
        self.size.get()
    }

    pub(super) fn render_card(&self, cx: &App) -> AnyElement {
        div()
            .font_family(UI_FONT_FAMILY)
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
