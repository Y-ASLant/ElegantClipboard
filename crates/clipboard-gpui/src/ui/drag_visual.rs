mod motion;

use super::*;

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
        if self.is_image {
            text.font_medium().child(tr(self.language, "图片", "Image"))
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
            .child(body)
            .child(self.metadata_row(cx))
            .when(self.is_pinned, |card| card.child(pin_ribbon(cx)))
    }
}

/// 置顶封条的平行斜边连接卡片上边与右边，留白等于卡片圆角。
fn pin_ribbon(cx: &App) -> Div {
    let inner = crate::card_radius::card_radius(cx).as_f32();
    let outer = inner + 12.;
    let fill = cx.theme().primary;
    div().absolute().top_0().right_0().size(px(outer)).child(
        canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                let corner = point(bounds.origin.x + bounds.size.width, bounds.origin.y);
                let mut path = PathBuilder::fill();
                path.move_to(point(corner.x - px(outer), corner.y));
                path.line_to(point(corner.x - px(inner), corner.y));
                path.line_to(point(corner.x, corner.y + px(inner)));
                path.line_to(point(corner.x, corner.y + px(outer)));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, fill);
                }
            },
        )
        .size_full(),
    )
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
        drag_card(self.size(), self.card.display.card_density, cx)
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
