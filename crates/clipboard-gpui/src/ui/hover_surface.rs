use super::*;
use gpui_kit::Size;

#[derive(Clone, Copy)]
enum RevealSide {
    Left,
    Right,
    Top,
    Bottom,
    Overlay,
}

pub(super) fn render_preview(
    view: &HoverPreviewWindowView,
    window: &mut Window,
    cx: &mut Context<HoverPreviewWindowView>,
) -> AnyElement {
    let geometry = view.zoom_geometry;
    let local =
        |bounds: Bounds<Pixels>| Bounds::new(bounds.origin - geometry.display.origin, bounds.size);
    let mut target = local(geometry.initial);
    let owner = local(geometry.owner);
    let source = local(view.source_bounds);
    let side = reveal_side(local(geometry.initial), owner);
    let anchor_y = geometry.anchor_y - geometry.display.origin.y;
    let viewport = window.viewport_size();
    let file_image = match &view.content {
        Ok(PreviewContent::Files(entries)) => single_file_image_path(entries),
        _ => None,
    };
    let image_path = match &view.content {
        Ok(PreviewContent::Image(path)) => Some(path.clone()),
        _ => file_image,
    };
    let image_preview = image_path.is_some();
    let border = if image_preview {
        px(0.)
    } else {
        px(visual::BORDER_WIDTH)
    };
    let available = available_size(target, side, viewport);
    let maximum = size(
        (available.width - border * 2.).max(px(1.)),
        (available.height - border * 2.).max(px(1.)),
    );
    if image_preview {
        target = local(geometry.scaled(view.zoom).0);
    } else {
        let Some(content) = hover_content::fitted_size(view, maximum, window, cx) else {
            return div()
                .size_full()
                .font_family(cx.theme().font_family.clone())
                .child(
                    div()
                        .absolute()
                        .opacity(0.)
                        .max_w(maximum.width)
                        .child(hover_content::html_content(view, maximum.width, cx)),
                )
                .into_any_element();
        };
        target = fitted_bounds(
            target,
            size(content.width + border * 2., content.height + border * 2.),
            side,
            anchor_y,
            viewport,
        );
    }
    let content_size = size(
        (target.size.width - border * 2.).max(px(1.)),
        (target.size.height - border * 2.).max(px(1.)),
    );
    let note_height = hover_content::note_height(
        view.information.note.as_ref(),
        size(
            content_size.width,
            if image_preview {
                content_size.height
            } else {
                maximum.height
            },
        ),
        window,
        cx,
    )
    .min((content_size.height - px(1.)).max(px(0.)));
    let body_size = size(
        content_size.width,
        (content_size.height - note_height).max(px(1.)),
    );
    let theme = cx.theme();
    let content = div()
        .flex_none()
        .flex()
        .flex_col()
        .w(content_size.width)
        .h(content_size.height)
        .when_some(view.information.note.clone(), |this, note| {
            this.child(
                div()
                    .flex_none()
                    .w_full()
                    .h(note_height)
                    .overflow_hidden()
                    .rounded_t((theme.radius_lg - border).max(px(0.)))
                    .border_b(px(visual::BORDER_WIDTH))
                    .border_color(theme.border)
                    .bg(theme.group_box)
                    .child(
                        div()
                            .id("hover-preview-note")
                            .size_full()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .px(px(visual::SPACE_3))
                                    .py(px(visual::SPACE_2))
                                    .text_size(px(visual::TEXT_BODY))
                                    .line_height(px(20.))
                                    .whitespace_normal()
                                    .child(note),
                            ),
                    ),
            )
        })
        .child(hover_content::render_content(
            view, image_path, body_size, cx,
        ));
    let surface = div()
        .size_full()
        .overflow_hidden()
        .rounded(theme.radius_lg)
        .when(!image_preview, |this| {
            this.border(px(visual::BORDER_WIDTH))
                .border_color(theme.border)
        })
        .bg(theme.popover)
        .shadow(visual::popover_shadow())
        .child(content);
    let entity = cx.entity().downgrade();
    let generation = view.generation;
    let measured = canvas(
        move |bounds, window, cx| {
            let Some(entity) = entity.upgrade() else {
                return;
            };
            let view = entity.read(cx);
            if view.generation != generation || view.surface_bounds == Some(bounds) {
                return;
            }
            window.on_next_frame(move |_, cx| {
                entity.update(cx, |view, cx| {
                    if view.generation == generation && view.surface_bounds != Some(bounds) {
                        view.surface_bounds = Some(bounds);
                        cx.notify();
                    }
                });
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
    .size_full();
    let surface = div()
        .absolute()
        .left(target.left())
        .top(target.top())
        .w(target.size.width)
        .h(target.size.height)
        .child(surface)
        .child(measured);
    let surface = if cx.reduce_motion() {
        surface.into_any_element()
    } else {
        let animated = surface.with_animation(
            ("hover-preview-size", generation),
            Animation::new(crate::design_system::PREVIEW_REVEAL).with_easing(ease_out_quint()),
            move |this, progress| {
                let bounds = reveal_bounds(target, source.size, side, anchor_y, viewport, progress);
                this.left(bounds.left())
                    .top(bounds.top())
                    .w(bounds.size.width)
                    .h(bounds.size.height)
            },
        );
        animated.into_any_element()
    };
    let percentage = image_preview.then(|| {
        view.information
            .image_dimensions
            .filter(|(width, height)| *width > 0 && *height > 0)
            .map(|(width, height)| {
                (f32::from(body_size.width) / width as f32)
                    .min(f32::from(body_size.height) / height as f32)
                    * geometry.scale_factor
            })
            .unwrap_or(view.zoom * view.image_scale.unwrap_or(1.))
            .mul_add(100., 0.)
            .round() as i32
    });
    let preview = div()
        .relative()
        .size_full()
        .font_family(cx.theme().font_family.clone())
        .text_color(cx.theme().popover_foreground)
        .bg(transparent_black())
        .child(surface)
        .when_some(view.surface_bounds, |this, surface| {
            this.child(hover_curve::render_curve(source, surface, cx))
                .child(hover_information::render_information(
                    &view.information,
                    hover_information::InformationPlacement {
                        surface,
                        owner,
                        viewport,
                        percentage,
                        language: view.language,
                    },
                    window,
                    cx,
                ))
        });
    if cx.reduce_motion() {
        preview.into_any_element()
    } else {
        // One opacity timeline for the content, connector and metadata. Geometry
        // updates during zoom keep the generation and do not replay the entrance.
        preview
            .with_animation(
                ("hover-preview-opacity", generation),
                Animation::new(crate::design_system::PREVIEW_REVEAL).with_easing(ease_out_quint()),
                |this, progress| this.opacity(progress),
            )
            .into_any_element()
    }
}

fn reveal_side(target: Bounds<Pixels>, owner: Bounds<Pixels>) -> RevealSide {
    if target.right() <= owner.left() {
        RevealSide::Left
    } else if target.left() >= owner.right() {
        RevealSide::Right
    } else if target.bottom() <= owner.top() {
        RevealSide::Top
    } else if target.top() >= owner.bottom() {
        RevealSide::Bottom
    } else {
        RevealSide::Overlay
    }
}

fn available_size(
    target: Bounds<Pixels>,
    side: RevealSide,
    viewport: Size<Pixels>,
) -> Size<Pixels> {
    let width = match side {
        RevealSide::Left => target.right() - px(12.),
        RevealSide::Right => viewport.width - px(12.) - target.left(),
        _ => viewport.width - px(24.),
    };
    let height = match side {
        RevealSide::Top => target.bottom() - px(12.),
        RevealSide::Bottom => viewport.height - px(12.) - target.top(),
        _ => viewport.height - px(24.),
    };
    size(
        width.max(px(1.)),
        height.min(viewport.height * 0.8).max(px(1.)),
    )
}

fn fitted_bounds(
    target: Bounds<Pixels>,
    dimensions: Size<Pixels>,
    side: RevealSide,
    anchor_y: Pixels,
    viewport: Size<Pixels>,
) -> Bounds<Pixels> {
    let x = match side {
        RevealSide::Left => target.right() - dimensions.width,
        RevealSide::Right => target.left(),
        _ => target.left() + (target.size.width - dimensions.width) / 2.,
    };
    let y = match side {
        RevealSide::Top => target.bottom() - dimensions.height,
        RevealSide::Bottom | RevealSide::Overlay => target.top(),
        _ => anchor_y - dimensions.height / 2.,
    };
    Bounds::new(
        point(
            x.clamp(
                px(12.),
                (viewport.width - px(12.) - dimensions.width).max(px(12.)),
            ),
            y.clamp(
                px(12.),
                (viewport.height - px(12.) - dimensions.height).max(px(12.)),
            ),
        ),
        dimensions,
    )
}

fn reveal_bounds(
    target: Bounds<Pixels>,
    source: Size<Pixels>,
    side: RevealSide,
    anchor_y: Pixels,
    viewport: Size<Pixels>,
    progress: f32,
) -> Bounds<Pixels> {
    let start = match side {
        RevealSide::Left | RevealSide::Right => {
            size(px(0.), source.height.max(px(1.)).min(target.size.height))
        }
        RevealSide::Top | RevealSide::Bottom => {
            size(source.width.max(px(1.)).min(target.size.width), px(0.))
        }
        RevealSide::Overlay => size(px(0.), px(0.)),
    };
    let dimensions = size(
        start.width + (target.size.width - start.width) * progress,
        start.height + (target.size.height - start.height) * progress,
    );
    let origin = match side {
        RevealSide::Left | RevealSide::Right => point(
            if matches!(side, RevealSide::Left) {
                target.right() - dimensions.width
            } else {
                target.left()
            },
            (anchor_y - dimensions.height / 2.).clamp(
                px(12.),
                (viewport.height - px(12.) - dimensions.height).max(px(12.)),
            ),
        ),
        RevealSide::Top | RevealSide::Bottom | RevealSide::Overlay => point(
            target.left() + (target.size.width - dimensions.width) / 2.,
            if matches!(side, RevealSide::Top) {
                target.bottom() - dimensions.height
            } else {
                target.top()
            },
        ),
    };
    Bounds::new(origin, dimensions)
}

#[cfg(test)]
mod tests {
    use super::{
        Bounds, RevealSide, available_size, fitted_bounds, point, px, reveal_bounds, size,
    };

    #[test]
    fn short_content_keeps_facing_edge_without_fixed_height_padding() {
        let initial = Bounds::new(point(px(300.), px(200.)), size(px(460.), px(300.)));
        let fitted = fitted_bounds(
            initial,
            size(px(80.), px(48.)),
            RevealSide::Left,
            px(350.),
            size(px(1920.), px(1080.)),
        );
        assert_eq!(fitted.right(), initial.right());
        assert_eq!(fitted.size, size(px(80.), px(48.)));
        assert_eq!(fitted.top(), px(326.));
    }

    #[test]
    fn side_reveal_clamps_source_height_and_finishes_at_fitted_surface() {
        let viewport = size(px(1920.), px(1080.));
        let target = Bounds::new(point(px(80.), px(12.)), size(px(320.), px(200.)));
        let source = size(px(460.), px(500.));
        let start = reveal_bounds(target, source, RevealSide::Left, px(40.), viewport, 0.);
        assert_eq!(start.size, size(px(0.), px(200.)));
        assert_eq!(start.right(), target.right());
        assert_eq!(start.top(), px(12.));
        assert_eq!(
            reveal_bounds(target, source, RevealSide::Left, px(40.), viewport, 1.),
            target,
        );
    }

    #[test]
    fn vertical_surface_capacity_preserves_facing_boundary() {
        let viewport = size(px(1000.), px(800.));
        let initial = Bounds::new(point(px(300.), px(20.)), size(px(400.), px(300.)));
        let maximum = available_size(initial, RevealSide::Top, viewport);
        assert_eq!(maximum.height, px(308.));
        let fitted = fitted_bounds(initial, maximum, RevealSide::Top, px(400.), viewport);
        assert_eq!(fitted.top(), px(12.));
        assert_eq!(fitted.bottom(), initial.bottom());
        let start = reveal_bounds(
            fitted,
            size(px(120.), px(80.)),
            RevealSide::Top,
            px(400.),
            viewport,
            0.,
        );
        assert_eq!(start.size, size(px(120.), px(0.)));
        assert_eq!(start.bottom(), fitted.bottom());
    }
}
