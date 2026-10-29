use super::*;

#[derive(Clone, Copy)]
struct Anchor {
    point: Point<f32>,
    normal: Point<f32>,
}

#[derive(Clone, Copy)]
struct Curve {
    source: Point<f32>,
    target: Point<f32>,
    control_1: Point<f32>,
    control_2: Point<f32>,
}

pub(super) fn render_curve(source: Bounds<Pixels>, target: Bounds<Pixels>, cx: &App) -> AnyElement {
    if source.size.width <= px(0.) || source.size.height <= px(0.) {
        return div().into_any_element();
    }
    let curve = nearest_curve(source, target);
    let primary = cx.theme().primary.opacity(0.65);
    let endpoint = cx.theme().primary_active;
    let dot = |position: Point<f32>| {
        div()
            .absolute()
            .left(px(position.x - 4.))
            .top(px(position.y - 4.))
            .size(px(8.))
            .rounded_full()
            .bg(endpoint.opacity(0.14))
            .child(div().absolute().inset(px(2.)).rounded_full().bg(endpoint))
    };
    div()
        .absolute()
        .inset_0()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, _| {
                    let local =
                        |p: Point<f32>| point(bounds.origin.x + px(p.x), bounds.origin.y + px(p.y));
                    let mut path = PathBuilder::stroke(px(1.5));
                    path.move_to(local(curve.source));
                    path.cubic_bezier_to(
                        local(curve.target),
                        local(curve.control_1),
                        local(curve.control_2),
                    );
                    if let Ok(path) = path.build() {
                        window.paint_path(path, primary);
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
        .child(dot(curve.source))
        .child(dot(curve.target))
        .into_any_element()
}

fn anchors(rect: Bounds<Pixels>) -> [Anchor; 4] {
    let left = f32::from(rect.left());
    let right = f32::from(rect.right());
    let top = f32::from(rect.top());
    let bottom = f32::from(rect.bottom());
    [
        Anchor {
            point: point(left, (top + bottom) / 2.),
            normal: point(-1., 0.),
        },
        Anchor {
            point: point(right, (top + bottom) / 2.),
            normal: point(1., 0.),
        },
        Anchor {
            point: point((left + right) / 2., top),
            normal: point(0., -1.),
        },
        Anchor {
            point: point((left + right) / 2., bottom),
            normal: point(0., 1.),
        },
    ]
}

fn crosses_interior(start: Point<f32>, end: Point<f32>, rect: Bounds<Pixels>) -> bool {
    let left = f32::from(rect.left()) + 6.;
    let right = f32::from(rect.right()) - 6.;
    let top = f32::from(rect.top()) + 6.;
    let bottom = f32::from(rect.bottom()) - 6.;
    (1..24).any(|step| {
        let progress = step as f32 / 24.;
        let x = start.x + (end.x - start.x) * progress;
        let y = start.y + (end.y - start.y) * progress;
        x > left && x < right && y > top && y < bottom
    })
}

fn nearest_curve(source_rect: Bounds<Pixels>, target_rect: Bounds<Pixels>) -> Curve {
    let mut best: Option<(f32, Curve)> = None;
    for source in anchors(source_rect) {
        for target in anchors(target_rect) {
            let dx = target.point.x - source.point.x;
            let dy = target.point.y - source.point.y;
            let distance = dx.hypot(dy);
            let source_outward = dx * source.normal.x + dy * source.normal.y >= 0.;
            let target_outward = -dx * target.normal.x - dy * target.normal.y >= 0.;
            let score = if crosses_interior(source.point, target.point, target_rect) {
                100_000.
            } else {
                0.
            } + if source_outward { 0. } else { 40. }
                + if target_outward { 0. } else { 40. }
                + distance;
            if best.is_none_or(|(best_score, _)| score < best_score) {
                // Keep both handles within half the facing gap. A fixed minimum
                // makes short connectors bulge or double back near the endpoints.
                let source_gap = (dx * source.normal.x + dy * source.normal.y).max(0.);
                let target_gap = (-dx * target.normal.x - dy * target.normal.y).max(0.);
                let control = (distance * 0.22)
                    .min(64.)
                    .min(source_gap * 0.5)
                    .min(target_gap * 0.5);
                best = Some((
                    score,
                    Curve {
                        source: source.point,
                        target: target.point,
                        control_1: point(
                            source.point.x + source.normal.x * control,
                            source.point.y + source.normal.y * control,
                        ),
                        control_2: point(
                            target.point.x + target.normal.x * control,
                            target.point.y + target.normal.y * control,
                        ),
                    },
                ));
            }
        }
    }
    best.expect("rectangles always provide four edge anchors").1
}

#[cfg(test)]
mod tests {
    use super::nearest_curve;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn narrow_side_connectors_do_not_double_back_between_edges() {
        let source = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(400.)));
        for (x, direction) in [(108., 1.), (-108., -1.)] {
            let target = Bounds::new(point(px(x), px(30.)), source.size);
            let curve = nearest_curve(source, target);
            let along = |x: f32| (x - curve.source.x) * direction;
            let first = along(curve.control_1.x);
            let second = along(curve.control_2.x);
            let end = along(curve.target.x);
            assert!(end > 0., "connector must face the other surface");
            assert!(first >= 0. && first <= second && second <= end);
            assert_eq!(curve.control_1.y, curve.source.y);
            assert_eq!(curve.control_2.y, curve.target.y);
        }
    }
}
