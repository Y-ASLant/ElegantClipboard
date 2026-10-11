use super::*;
fn hover_zoom_percent(current: u16, change: i32) -> u16 {
    (i32::from(current) + change).clamp(50, 400) as u16
}

impl HoverPreviewWindowView {
    fn change_zoom(&mut self, change: i32, cx: &mut Context<Self>) {
        let next = hover_zoom_percent(self.zoom_percent, change);
        if next != self.zoom_percent {
            self.zoom_percent = next;
            cx.notify();
        }
    }

    pub(super) fn new(
        owner: WeakEntity<ClipboardView>,
        request: (i64, u64),
        language: LanguagePreference,
        content: Result<PreviewContent, String>,
        zoom_step: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = match &content {
            Ok(PreviewContent::Text(text) | PreviewContent::RichText(text)) => text.clone(),
            Ok(PreviewContent::Files(entries)) => entries
                .iter()
                .map(|entry| entry.original_path.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => error.clone(),
            Ok(PreviewContent::Image(_)) => String::new(),
        };
        let text_input = cx.new(|cx| TextareaState::new(window, cx));
        text_input.update(cx, |input, cx| input.set_value(text, window, cx));
        Self {
            owner,
            id: request.0,
            generation: request.1,
            language,
            content,
            text_input,
            zoom_percent: 100,
            image_scroll: ScrollHandle::default(),
            zoom_step,
        }
    }
}

impl Render for HoverPreviewWindowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match &self.content {
            Ok(PreviewContent::Text(_)) => tr(self.language, "文本预览", "Text preview"),
            Ok(PreviewContent::RichText(_)) => tr(self.language, "富文本预览", "Rich-text preview"),
            Ok(PreviewContent::Image(_)) => tr(self.language, "图片预览", "Image preview"),
            Ok(PreviewContent::Files(_)) => tr(self.language, "文件预览", "File preview"),
            Err(_) => tr(self.language, "预览失败", "Preview unavailable"),
        };
        let file_image_path = match &self.content {
            Ok(PreviewContent::Files(entries)) => single_file_image_path(entries),
            _ => None,
        };
        let image_preview =
            matches!(&self.content, Ok(PreviewContent::Image(_))) || file_image_path.is_some();
        let body: AnyElement = match &self.content {
            Ok(PreviewContent::Image(path)) => div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("hover-image-viewport")
                        .size_full()
                        .overflow_scroll()
                        .track_scroll(&self.image_scroll)
                        .on_scroll_wheel(cx.listener(
                            |this, event: &ScrollWheelEvent, window, cx| {
                                if event.modifiers.control {
                                    let delta = event.delta.pixel_delta(window.line_height()).y;
                                    let change = if delta > px(0.) {
                                        i32::from(this.zoom_step)
                                    } else {
                                        -i32::from(this.zoom_step)
                                    };
                                    this.change_zoom(change, cx);
                                    cx.stop_propagation();
                                }
                            },
                        ))
                        .when(self.zoom_percent <= 100, |viewport| {
                            viewport.flex().items_center().justify_center()
                        })
                        .child(
                            div()
                                .w(relative(f32::from(self.zoom_percent) / 100.0))
                                .h(relative(f32::from(self.zoom_percent) / 100.0))
                                .flex_none()
                                .child(
                                    img(path.clone()).size_full().object_fit(ObjectFit::Contain),
                                ),
                        ),
                )
                .scrollbar(&self.image_scroll, ScrollbarAxis::Both)
                .into_any_element(),
            Ok(PreviewContent::Files(entries)) if file_image_path.is_some() => {
                let path = file_image_path.expect("checked above");
                let unavailable = tr(self.language, "图片无法显示", "Image unavailable");
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id("hover-file-image-viewport")
                                    .size_full()
                                    .overflow_scroll()
                                    .track_scroll(&self.image_scroll)
                                    .on_scroll_wheel(cx.listener(
                                        |this, event: &ScrollWheelEvent, window, cx| {
                                            if event.modifiers.control {
                                                let delta =
                                                    event.delta.pixel_delta(window.line_height()).y;
                                                let change = if delta > px(0.) {
                                                    i32::from(this.zoom_step)
                                                } else {
                                                    -i32::from(this.zoom_step)
                                                };
                                                this.change_zoom(change, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                                    .when(self.zoom_percent <= 100, |viewport| {
                                        viewport.flex().items_center().justify_center()
                                    })
                                    .child(
                                        div()
                                            .w(relative(f32::from(self.zoom_percent) / 100.0))
                                            .h(relative(f32::from(self.zoom_percent) / 100.0))
                                            .flex_none()
                                            .child(
                                                img(path)
                                                    .size_full()
                                                    .object_fit(ObjectFit::Contain)
                                                    .with_fallback(move || {
                                                        div().child(unavailable).into_any_element()
                                                    }),
                                            ),
                                    ),
                            )
                            .scrollbar(&self.image_scroll, ScrollbarAxis::Both),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .line_clamp(1)
                            .text_ellipsis()
                            .child(entries[0].original_path.clone()),
                    )
                    .into_any_element()
            }
            _ => Textarea::new(&self.text_input)
                .readonly(true)
                .size_full()
                .aria_label(title)
                .into_any_element(),
        };
        let owner = self.owner.clone();
        let id = self.id;
        let generation = self.generation;
        window_shell(cx)
            .id("hover-preview-window")
            .p_3()
            .gap_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .shadow_lg()
            .on_hover(move |hovered, _, cx| {
                let _ = owner.update(cx, |this, cx| {
                    this.set_hover_popup_active(id, generation, *hovered, cx);
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_sm()
                    .font_semibold()
                    .child(title)
                    .when(image_preview, |header| {
                        header.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new("hover-zoom-out")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Minus)
                                        .accessibility_label(tr(
                                            self.language,
                                            "缩小悬浮图片",
                                            "Zoom out hover image",
                                        ))
                                        .disabled(self.zoom_percent <= 50)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.change_zoom(-i32::from(this.zoom_step), cx);
                                        })),
                                )
                                .child(
                                    Button::new("hover-zoom-reset")
                                        .xsmall()
                                        .ghost()
                                        .label(format!("{}%", self.zoom_percent))
                                        .accessibility_label(tr(
                                            self.language,
                                            "重置悬浮图片缩放",
                                            "Reset hover image zoom",
                                        ))
                                        .disabled(self.zoom_percent == 100)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.zoom_percent = 100;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("hover-zoom-in")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Plus)
                                        .accessibility_label(tr(
                                            self.language,
                                            "放大悬浮图片",
                                            "Zoom in hover image",
                                        ))
                                        .disabled(self.zoom_percent >= 400)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.change_zoom(i32::from(this.zoom_step), cx);
                                        })),
                                ),
                        )
                    }),
            )
            .child(div().flex_1().min_h_0().child(body))
    }
}
#[cfg(test)]
mod hover_zoom_tests {
    use super::hover_zoom_percent;

    #[test]
    fn zoom_stays_within_visible_range() {
        assert_eq!(hover_zoom_percent(100, 20), 120);
        assert_eq!(hover_zoom_percent(50, -50), 50);
        assert_eq!(hover_zoom_percent(400, 50), 400);
    }
}
