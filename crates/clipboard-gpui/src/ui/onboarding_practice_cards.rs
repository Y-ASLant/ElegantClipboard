use super::*;
use gpui_kit::component::text::TextView;

impl OnboardingPractice {
    fn highlight(&self, text: &'static str, query: &str, cx: &App) -> StyledText {
        let highlights = search_highlight_ranges(text, query)
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
            .collect::<Vec<_>>();
        StyledText::new(text).with_highlights(highlights)
    }

    pub(super) fn card(&self, sample: Sample, query: &str, cx: &mut Context<Self>) -> AnyElement {
        let language = self.language;
        let card = div()
            .id(match sample {
                Sample::Text => "demo-text",
                Sample::Image => "demo-image",
                Sample::RichText => "demo-rich",
            })
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(px(visual::SPACE_3))
            .p(px(visual::SPACE_2))
            .rounded(cx.theme().radius_lg)
            .border(px(visual::BORDER_WIDTH))
            .border_color(crate::design_system::container_border(cx))
            .bg(cx.theme().group_box);
        if matches!(sample, Sample::Image) {
            let source = self.source_bounds.clone();
            let unavailable = crate::capabilities::positioning_unavailable_reason(language);
            return card
                .child(
                    img(self.image.clone())
                        .w(px(88.))
                        .h(px(60.))
                        .flex_none()
                        .object_fit(ObjectFit::Cover)
                        .rounded(cx.theme().radius),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(visual::SPACE_1))
                        .child(
                            div()
                                .font_medium()
                                .text_size(px(visual::TEXT_SMALL))
                                .child(self.highlight(sample.label(language), query, cx)),
                        )
                        .child(
                            div()
                                .text_size(px(visual::TEXT_SMALL))
                                .whitespace_normal()
                                .child(self.highlight(
                                    unavailable.unwrap_or(sample.text(language)),
                                    query,
                                    cx,
                                )),
                        ),
                )
                .child(
                    canvas(
                        move |bounds, _, _| source.set(Some(bounds)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                )
                .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                    if *hovered && unavailable.is_none() {
                        this.open_preview(window, cx);
                    } else {
                        this.close_preview(cx);
                    }
                }))
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                        if unavailable.is_none() {
                            this.open_preview(window, cx);
                        }
                        this.update_anchor(event.position.y, cx);
                    }),
                )
                .on_scroll(cx.listener(|this, event: &ScrollEvent, window, cx| {
                    if event.modifiers.control {
                        if let Some(handle) = this.popup {
                            let delta = event.delta.pixel_delta(window.line_height()).y;
                            if handle
                                .update(cx, |preview, window, cx| {
                                    preview.zoom_by_wheel(delta, window, cx)
                                })
                                .unwrap_or(false)
                            {
                                cx.stop_propagation();
                            }
                        }
                    } else {
                        this.close_preview(cx);
                    }
                }))
                .into_any_element();
        }
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_1))
            .child(
                div()
                    .font_medium()
                    .text_size(px(visual::TEXT_SMALL))
                    .child(self.highlight(sample.label(language), query, cx)),
            )
            .when(
                matches!(sample, Sample::RichText) && query.trim().is_empty(),
                |content| {
                    content.child(
                        TextView::html("practice-rich-html", Sample::html(language))
                            .selectable(false)
                            .scrollable(false),
                    )
                },
            )
            .when(
                !matches!(sample, Sample::RichText) || !query.trim().is_empty(),
                |content| {
                    content.child(
                        div()
                            .text_size(px(visual::TEXT_BODY))
                            .whitespace_normal()
                            .child(self.highlight(sample.text(language), query, cx)),
                    )
                },
            );
        card.cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| this.insert(sample, window, cx)))
            .child(text)
            .child(
                Button::new(match sample {
                    Sample::Text => "insert-demo-text",
                    _ => "insert-demo-plain",
                })
                .small()
                .ghost()
                .flex_none()
                .label(if matches!(sample, Sample::RichText) {
                    tr(language, "去格式插入", "Plain text")
                } else {
                    tr(language, "插入", "Insert")
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.insert(sample, window, cx);
                })),
            )
            .into_any_element()
    }
}
