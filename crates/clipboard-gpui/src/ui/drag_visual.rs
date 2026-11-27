mod motion;

use super::*;
use gpui_kit::component::tag::Tag;

#[derive(Clone)]
pub(super) struct CardText {
    text: SharedString,
    highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
}

impl CardText {
    pub(super) fn plain(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            highlights: Vec::new(),
        }
    }

    pub(super) fn searched(text: impl Into<SharedString>, search: &str, cx: &App) -> Self {
        let mut label = Self::plain(text);
        label.highlights = clipboard_core::search::search_highlight_ranges(&label.text, search)
            .into_iter()
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        color: Some(cx.theme().blue),
                        background_color: Some(cx.theme().primary.opacity(0.18)),
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        label
    }

    fn styled(&self) -> StyledText {
        StyledText::new(self.text.clone()).with_highlights(self.highlights.clone())
    }
}

#[derive(Clone)]
pub(super) struct HistoryCardSnapshot {
    pub(super) is_image: bool,
    pub(super) is_files: bool,
    pub(super) is_pinned: bool,
    pub(super) card_index: usize,
    pub(super) source_app_name: Option<String>,
    pub(super) source_app_icon: Option<String>,
    pub(super) display: DisplayPreference,
    pub(super) language: LanguagePreference,
    pub(super) summary: String,
    pub(super) title: CardText,
    pub(super) is_note: bool,
    pub(super) source_match: Option<CardText>,
    pub(super) thumbnail_path: Option<PathBuf>,
    // Cloning a drag snapshot also retains the decoded pixels after list eviction.
    pub(super) thumbnail: Option<gpui_kit::ImageSource>,
    pub(super) file_warning: bool,
    pub(super) file_icon: IconName,
    pub(super) selected: bool,
}

impl HistoryCardSnapshot {
    fn metadata_row(&self, drop_hint: Option<&'static str>, cx: &App) -> Div {
        let source_icon = self
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) =
            source_app_parts(self.display.source_app_display, source_icon.is_some());
        let summary = drop_hint.map_or_else(|| self.summary.clone().into(), SharedString::from);
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
                    .when(drop_hint.is_some(), |summary| {
                        summary.text_color(cx.theme().accent_foreground)
                    })
                    .child(summary),
            )
            .when(self.display.show_source_app, |footer| {
                footer.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(visual::SPACE_1))
                        .max_w(px(200.))
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
                            let name = self
                                .source_app_name
                                .as_deref()
                                .map(|name| source_app_visible_name(name, false).to_owned())
                                .unwrap_or_default();
                            source.child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(name),
                            )
                        }),
                )
            })
            .when(self.display.show_card_number, |footer| {
                footer.child(
                    div()
                        .flex_none()
                        .min_w(px(visual::SPACE_6))
                        .h(px(visual::SPACE_4))
                        .px(px(visual::SPACE_1))
                        .rounded_full()
                        .bg(cx.theme().primary.opacity(0.12))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(cx.theme().primary)
                        .child((self.card_index + 1).to_string()),
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
            .rounded(crate::card_radius::card_radius(cx))
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
        let fonts = crate::design_system::active_fonts(cx);
        let text = div()
            .min_w_0()
            .font_family(crate::fonts::resolve_card_family(&fonts))
            .text_size(px(f32::from(fonts.card_font_size)))
            .line_height(relative(visual::COPY_LINE_HEIGHT))
            .line_clamp(self.display.card_max_lines as usize)
            .text_ellipsis()
            .text_color(if self.file_warning {
                cx.theme().danger
            } else {
                cx.theme().foreground
            });
        let title = if self.is_image && !self.is_note {
            text.font_medium().child(tr(self.language, "图片", "Image"))
        } else {
            text.child(self.title.styled())
        };
        if !self.is_note {
            return title;
        }
        let note = div()
            .flex()
            .items_start()
            .gap(px(visual::SPACE_1))
            .child(
                Icon::new(IconName::FileText)
                    .size(px(visual::ICON_SIZE))
                    .text_color(cx.theme().muted_foreground),
            )
            .child(title.flex_1());
        div().min_w_0().flex().flex_col().child(note).when_some(
            self.source_match.as_ref(),
            |column, source| {
                column.child(
                    div()
                        .min_w_0()
                        .h(px(visual::PREVIEW_LINE_HEIGHT))
                        .line_clamp(1)
                        .text_ellipsis()
                        .text_size(px(visual::TEXT_SMALL))
                        .text_color(cx.theme().muted_foreground)
                        .child(source.styled()),
                )
            },
        )
    }

    pub(super) fn surface(
        &self,
        preview: bool,
        show_pin: bool,
        drop_hint: Option<&'static str>,
        cx: &App,
    ) -> Div {
        let media_layout = self.is_image || self.is_files;
        let spacing = visual::card_spacing(self.display.card_density);
        let padding = visual::card_padding(self.display.card_density);
        let body = div().min_w_0().overflow_hidden().flex().items_start();
        // The summary stays on its own row. Beside a thumbnail it wraps mid-token
        // ("今天 20" / "24 · 文件") once the source name takes the remaining width.
        let body = if media_layout {
            body.gap(px(visual::SPACE_3))
                .child(self.media_visual(preview, cx))
                .child(self.content_text(cx).flex_1().overflow_hidden())
        } else {
            body.flex_1()
                .min_h_0()
                .child(self.content_text(cx).flex_1())
        };
        div()
            .h_full()
            .group("")
            .p(px(padding))
            .rounded(crate::card_radius::card_radius(cx))
            .overflow_hidden()
            .border(px(visual::BORDER_WIDTH))
            .border_color(if self.selected {
                cx.theme().list_active_border
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
            .child(body.when(self.is_pinned, |body| {
                let theme = cx.theme();
                body.child(
                    Tag::custom(
                        theme.accent,
                        theme.accent_foreground,
                        theme.ring.opacity(0.6),
                    )
                    .small()
                    .rounded(theme.radius)
                    .h(px(visual::METADATA_ROW_HEIGHT))
                    .px(px(visual::SPACE_3 / 2.))
                    .py(px(0.))
                    .flex_none()
                    .text_size(px(visual::TEXT_SMALL))
                    .whitespace_nowrap()
                    .when(!media_layout, |tag| tag.ml(px(visual::SPACE_2)))
                    .when(!show_pin, |tag| tag.invisible())
                    .child(tr(self.language, "置顶", "Pinned")),
                )
            }))
            .child(self.metadata_row(drop_hint, cx))
    }
}

// 尺寸与内边距跟随源行的实测几何，原生浮窗与 GPUI 回退共用同一外壳。
pub(super) fn drag_card(dimensions: gpui_kit::Size<Pixels>, density: CardDensity, cx: &App) -> Div {
    div()
        .font_family(crate::design_system::ui_font_family(cx))
        .w(dimensions.width)
        .h(dimensions.height)
        .px(px(PAGE_PADDING))
        .py(px(visual::card_spacing(density)))
        .text_color(cx.theme().foreground)
}

impl HistoryDrag {
    pub(super) fn size(&self) -> gpui_kit::Size<Pixels> {
        self.size.get()
    }

    pub(super) fn render_card(&self, cx: &App) -> AnyElement {
        // The preview can cover the strip, so keep its release hint visible here.
        let drop_hint = self
            .favorite_drop
            .hovered
            .get()
            .then(|| favorite_drop::release_hint(self.card.language, self.favorite_only));
        drag_card(self.size(), self.card.display.card_density, cx)
            .child(self.card.surface(true, true, drop_hint, cx))
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
