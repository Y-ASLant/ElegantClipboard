use super::*;

/// 缩放上下限（倍率，1.0 = 100%）。
const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 4.0;
/// 滚轮折算成缩放档位时的分母（逻辑像素）。一次事件最多走 1.5 档，
/// 免得不同平台/驱动上报的 delta 量级不同导致缩放跳变。
const WHEEL_ZOOM_UNIT: f32 = 60.;
const WHEEL_ZOOM_MAX_STEPS: f32 = 1.5;
/// 每档缩放的倍率。
const WHEEL_ZOOM_RATIO: f32 = 1.1;

/// 缩放后把视口中心对齐到缩放前中心所对应的内容点，返回新的滚动偏移。
///
/// `viewport` 是视口边长，`offset` 是当前偏移（非正值，0 表示内容顶端对齐），
/// `ratio` 是本次缩放倍率，`zoom_after` 是缩放后的总倍率——用它夹住偏移，
/// 避免内容小于视口时把图片推出可视区。
fn zoom_centered_offset(viewport: f32, offset: f32, ratio: f32, zoom_after: f32) -> f32 {
    if viewport <= 0. {
        return 0.;
    }
    let anchored = viewport / 2. - (viewport / 2. - offset) * ratio;
    anchored.clamp(-(viewport * (zoom_after - 1.).max(0.)), 0.)
}

impl HoverPreviewWindowView {
    /// 设置缩放倍率；保持视口中心不动，并把滚动偏移夹回合法区间。
    fn apply_zoom(&mut self, target: f32, cx: &mut Context<Self>) {
        let next = target.clamp(MIN_ZOOM, MAX_ZOOM);
        if (next - self.zoom).abs() < 1e-4 {
            return;
        }
        let ratio = next / self.zoom;
        let viewport = self.image_scroll.bounds().size;
        let offset = self.image_scroll.offset();
        self.zoom = next;
        self.image_scroll.set_offset(point(
            px(zoom_centered_offset(
                f32::from(viewport.width),
                f32::from(offset.x),
                ratio,
                next,
            )),
            px(zoom_centered_offset(
                f32::from(viewport.height),
                f32::from(offset.y),
                ratio,
                next,
            )),
        ));
        cx.notify();
    }

    /// 按钮缩放：按用户设定的步进百分比加减。
    fn step_zoom(&mut self, direction: f32, cx: &mut Context<Self>) {
        let step = f32::from(self.zoom_step) / 100.;
        self.apply_zoom(self.zoom + direction * step, cx);
    }

    /// 滚轮缩放：连续无极，不跳档。
    fn zoom_by_wheel(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let steps =
            (f32::from(delta) / WHEEL_ZOOM_UNIT).clamp(-WHEEL_ZOOM_MAX_STEPS, WHEEL_ZOOM_MAX_STEPS);
        if steps == 0. {
            return;
        }
        self.apply_zoom(self.zoom * WHEEL_ZOOM_RATIO.powf(steps), cx);
    }

    /// 记下拖拽起点与当时的偏移，供后续平移使用。
    fn begin_pan(&mut self, position: Point<Pixels>) {
        self.pan_from = Some((position, self.image_scroll.offset()));
    }

    /// 按当前光标位置平移图片（仅在内容大于视口时有效）。
    fn pan_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((origin, offset)) = self.pan_from else {
            return;
        };
        let delta = position - origin;
        let viewport = self.image_scroll.bounds().size;
        let slack_x = f32::from(viewport.width) * (self.zoom - 1.).max(0.);
        let slack_y = f32::from(viewport.height) * (self.zoom - 1.).max(0.);
        self.image_scroll.set_offset(point(
            (offset.x - delta.x).clamp(px(-slack_x), px(0.)),
            (offset.y - delta.y).clamp(px(-slack_y), px(0.)),
        ));
        cx.notify();
    }

    /// 图片预览主体：滚轮无极缩放 + 左键拖拽平移。
    fn image_viewport(&self, path: PathBuf, cx: &mut Context<Self>) -> AnyElement {
        let zoom = self.zoom;
        div()
            .id("hover-image-viewport")
            .size_full()
            .overflow_scroll()
            .track_scroll(&self.image_scroll)
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.begin_pan(event.position);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.pan_from = None),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    this.pan_from = None;
                    return;
                }
                this.pan_to(event.position, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let delta = event.delta.pixel_delta(window.line_height()).y;
                this.zoom_by_wheel(delta, cx);
                cx.stop_propagation();
            }))
            .when(zoom <= 1., |viewport| {
                viewport.flex().items_center().justify_center()
            })
            .child(
                div()
                    .w(relative(zoom))
                    .h(relative(zoom))
                    .flex_none()
                    .child(img(path).size_full().object_fit(ObjectFit::Contain)),
            )
            .into_any_element()
    }

    pub(super) fn new(
        owner: WeakEntity<ClipboardView>,
        request: (i64, u64),
        language: LanguagePreference,
        content: Result<PreviewContent, String>,
        zoom_step: u8,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        let text: SharedString = match &content {
            Ok(PreviewContent::Text(text) | PreviewContent::RichText(text)) => text.clone().into(),
            Ok(PreviewContent::Files(entries)) => entries
                .iter()
                .map(|entry| entry.original_path.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
            Err(error) => error.clone().into(),
            Ok(PreviewContent::Image(_)) => SharedString::default(),
        };
        Self {
            owner,
            id: request.0,
            generation: request.1,
            language,
            content,
            text,
            zoom: 1.0,
            pan_from: None,
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
        let body: AnyElement =
            match &self.content {
                Ok(PreviewContent::Image(path)) => self.image_viewport(path.clone(), cx),
                Ok(PreviewContent::Files(entries)) if file_image_path.is_some() => {
                    let unavailable = tr(self.language, "图片无法显示", "Image unavailable");
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().relative().flex_1().min_h_0().child(
                            self.image_viewport(file_image_path.expect("checked above"), cx),
                        ))
                        .child(
                            div()
                                .flex_none()
                                .text_xs()
                                .line_clamp(1)
                                .text_ellipsis()
                                .child(entries.first().map_or_else(
                                    || unavailable.to_owned(),
                                    |entry| entry.original_path.clone(),
                                )),
                        )
                        .into_any_element()
                }
                _ => div()
                    .id("hover-text-body")
                    .size_full()
                    .overflow_scroll()
                    .cursor_grab()
                    .window_control_area(WindowControlArea::Drag)
                    .text_sm()
                    .whitespace_normal()
                    .aria_label(title)
                    .child(self.text.clone())
                    .into_any_element(),
            };
        let owner = self.owner.clone();
        let id = self.id;
        let generation = self.generation;
        let zoom_percent = (self.zoom * 100.).round() as i32;
        window_shell(cx)
            .id("hover-preview-window")
            // 以前由 gpui-base 的 Root 统一设字体；现在这个窗口不走 Root（见
            // `hover_lifecycle.rs` 里绕开 window_border 的说明），这里显式对齐主窗口。
            .font_family("Microsoft YaHei UI")
            .p_3()
            .gap_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
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
                    .gap_2()
                    .text_sm()
                    .font_semibold()
                    .child(
                        // 标题这块就是浮窗的「标题栏」：按住可以拖动整个预览框，
                        // 免得它盖住任务栏或别的可点区域时没法挪开。
                        div()
                            .id("hover-drag-handle")
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .cursor_grab()
                            .window_control_area(WindowControlArea::Drag)
                            .child(title),
                    )
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
                                        .disabled(self.zoom <= MIN_ZOOM + 1e-3)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.step_zoom(-1., cx);
                                        })),
                                )
                                .child(
                                    Button::new("hover-zoom-reset")
                                        .xsmall()
                                        .ghost()
                                        .label(format!("{zoom_percent}%"))
                                        .accessibility_label(tr(
                                            self.language,
                                            "重置悬浮图片缩放",
                                            "Reset hover image zoom",
                                        ))
                                        .disabled((self.zoom - 1.).abs() < 1e-3)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.apply_zoom(1., cx);
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
                                        .disabled(self.zoom >= MAX_ZOOM - 1e-3)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.step_zoom(1., cx);
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
    use super::zoom_centered_offset;

    #[test]
    fn zooming_keeps_the_viewport_center_on_the_same_content_point() {
        // 视口 400，偏移 -100（内容顶端在视口上方 100px），放大到 2 倍。
        // 视口中心原本对应内容坐标 200 + 100 = 300；放大后该点应在 600，
        // 于是偏移应为 200 - 600 = -400。
        assert!((zoom_centered_offset(400., -100., 2.0, 2.0) - (-400.)).abs() < 0.01);
        // 缩回 1 倍时内容不比视口大，偏移回到 0。
        assert_eq!(zoom_centered_offset(400., -400., 0.5, 1.0), 0.);
        // 视口尺寸未知（尚未绘制）时不改动偏移。
        assert_eq!(zoom_centered_offset(0., -50., 2.0, 2.0), 0.);
    }

    #[test]
    fn centered_offset_never_overscrolls() {
        // 偏移不会被推到正值，也不会超出内容多出来的那部分。
        for offset in [0., -10., -399., -400.] {
            let next = zoom_centered_offset(400., offset, 1.25, 1.25);
            assert!(next <= 0.0);
            assert!(next >= -400. * 0.25 - 0.01);
        }
    }
}
