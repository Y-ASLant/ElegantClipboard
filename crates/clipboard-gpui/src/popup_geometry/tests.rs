use super::*;

#[test]
fn card_anchor_and_work_area_remain_stable_through_zoom_limits() {
    let display = Bounds::new(point(px(-1920.), px(-900.)), size(px(1920.), px(1040.)));
    let owner = Bounds::new(point(px(-500.), px(-800.)), size(px(480.), px(800.)));
    for anchor_y in [px(-850.), px(-400.), px(100.)] {
        let initial = hover_popup_bounds(
            owner,
            display,
            HoverPreviewPosition::Auto,
            Some((600, 800)),
            false,
            anchor_y,
        )
        .unwrap();
        let geometry = PopupZoomGeometry {
            initial,
            owner,
            display,
            anchor_y,
            scale_factor: 1.5,
        };
        assert_eq!(geometry.scaled(1.).0, initial);
        if anchor_y == px(-400.) {
            assert_eq!(initial.top() + initial.size.height / 2., anchor_y);
        }
        let (maximum, limit) = geometry.scaled(100.);
        assert!(maximum.top() >= display.top() + px(EDGE_INSET));
        assert!(maximum.bottom() <= display.bottom() - px(EDGE_INSET));
        assert!(maximum.right() < owner.left());
        let (smaller, actual) = geometry.scaled(limit / 1.1);
        assert!(actual < limit);
        assert!(smaller.size.height < maximum.size.height);
    }
}

#[test]
fn image_preview_rejects_work_area_without_room_for_its_surface() {
    let display = Bounds::new(point(px(-1000.), px(0.)), size(px(1000.), px(30.)));
    let owner = Bounds::new(point(px(-400.), px(0.)), size(px(300.), px(30.)));
    assert!(
        hover_popup_bounds(
            owner,
            display,
            HoverPreviewPosition::Auto,
            Some((600, 800)),
            false,
            px(15.),
        )
        .is_none()
    );
}

#[test]
fn window_zoom_grows_outward_and_clamps_without_cropping() {
    let display = Bounds::new(point(px(-1920.), px(0.)), size(px(1920.), px(1080.)));
    for (owner_x, initial_x, left) in [(-600., -1084., true), (-1800., -1216., false)] {
        let geometry = PopupZoomGeometry {
            initial: Bounds::new(point(px(initial_x), px(600.)), size(px(460.), px(300.))),
            owner: Bounds::new(point(px(owner_x), px(400.)), size(px(560.), px(600.))),
            display,
            anchor_y: px(750.),
            scale_factor: 1.,
        };
        let (grown, zoom) = geometry.scaled(1.5);
        assert_eq!(zoom, 1.5);
        assert_eq!(grown.size, size(px(690.), px(450.)));
        if left {
            assert_eq!(
                grown.origin.x + grown.size.width,
                geometry.initial.origin.x + px(460.)
            );
        } else {
            assert_eq!(grown.origin.x, geometry.initial.origin.x);
        }
        let (limited, actual) = geometry.scaled(100.);
        assert!(actual < 4.);
        assert!(limited.origin.x >= display.origin.x);
        assert!(limited.origin.x + limited.size.width <= px(0.));
        assert!(limited.origin.y + limited.size.height <= px(1080.));
        assert_eq!(limited.size.width / px(460.), actual);
        let (restored, zoom) = geometry.scaled(1.);
        assert_eq!(zoom, 1.);
        assert_eq!(restored, geometry.initial);
    }
}
#[test]
fn expanded_image_window_uses_dimensions_without_leaving_display() {
    let display = Bounds {
        origin: point(px(0.), px(0.)),
        size: size(px(1920.), px(1080.)),
    };
    let owner = Bounds::new(point(px(1400.), px(300.)), size(px(500.), px(760.)));
    let standard = hover_popup_bounds(
        owner,
        display,
        HoverPreviewPosition::Auto,
        None,
        false,
        px(680.),
    )
    .unwrap();

    // 中等图片：比默认大，完整落在显示区域内，且保持图片长宽比。
    let expanded = hover_popup_bounds(
        owner,
        display,
        HoverPreviewPosition::Auto,
        Some((1200, 800)),
        true,
        px(680.),
    )
    .unwrap();
    assert!(expanded.size.width > standard.size.width);
    assert!(expanded.size.height > standard.size.height);
    assert!(expanded.origin.x >= display.origin.x);
    assert!(expanded.origin.y >= display.origin.y);
    assert!(expanded.origin.x + expanded.size.width <= display.size.width);
    assert!(expanded.origin.y + expanded.size.height <= display.size.height);
    let desired_aspect = 1200. / 800.;
    let actual_aspect = expanded.size.width / expanded.size.height;
    assert!((actual_aspect - desired_aspect).abs() < 0.001);

    // 超大图片：收敛到显示区域的一小部分，不再铺满整屏，长宽比同样保持。
    let oversized = hover_popup_bounds(
        owner,
        display,
        HoverPreviewPosition::Auto,
        Some((5000, 5000)),
        true,
        px(680.),
    )
    .unwrap();
    assert!(f32::from(oversized.size.width) <= 1920. * EXPANDED_MAX_SHARE + 1.);
    assert!(f32::from(oversized.size.height) <= 1080. * EXPANDED_MAX_SHARE + 1.);
    assert!(oversized.size.width > standard.size.width);
    assert!(
        oversized.origin.x + oversized.size.width <= display.size.width
            && oversized.origin.y + oversized.size.height <= display.size.height
    );

    // 尺寸非法（0）时退回默认尺寸。
    assert_eq!(
        hover_popup_bounds(
            owner,
            display,
            HoverPreviewPosition::Auto,
            Some((0, 800)),
            true,
            px(680.),
        )
        .unwrap()
        .size,
        standard.size,
    );
}

#[test]
fn image_windows_keep_source_ratio_without_minimum_size_padding() {
    let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
    let owner = Bounds::new(point(px(1400.), px(0.)), size(px(500.), px(760.)));
    for dimensions in [(1600, 200), (200, 1600), (32, 24)] {
        for expanded in [false, true] {
            let popup = hover_popup_bounds(
                owner,
                display,
                HoverPreviewPosition::Auto,
                Some(dimensions),
                expanded,
                px(380.),
            )
            .unwrap();
            let ratio = popup.size.width / popup.size.height;
            assert!((ratio - dimensions.0 as f32 / dimensions.1 as f32).abs() < 0.001);
            assert!(popup.size.width > px(0.) && popup.size.height > px(0.));
            assert!(popup.left() >= display.left() && popup.right() <= display.right());
            let zoom = PopupZoomGeometry {
                initial: popup,
                owner,
                display,
                anchor_y: px(380.),
                scale_factor: 1.,
            };
            let (scaled, _) = zoom.scaled(3.);
            let scaled_ratio = scaled.size.width / scaled.size.height;
            assert!((scaled_ratio - ratio).abs() < 0.001);
        }
    }
}

#[test]
fn side_preview_flips_shrinks_and_never_overlaps_owner() {
    let display = Bounds::new(point(px(-1920.), px(40.)), size(px(1920.), px(1040.)));
    for (x, preference, left) in [
        (-600., HoverPreviewPosition::Right, true),
        (-1900., HoverPreviewPosition::Left, false),
        (-1300., HoverPreviewPosition::Auto, false),
    ] {
        let owner = Bounds::new(point(px(x), px(800.)), size(px(560.), px(240.)));
        let popup = hover_popup_bounds(
            owner,
            display,
            preference,
            Some((5000, 3000)),
            true,
            px(920.),
        )
        .unwrap();
        assert!(popup.origin.x >= display.origin.x);
        assert!(popup.origin.x + popup.size.width <= px(0.));
        assert!(popup.origin.y >= px(40.));
        assert!(popup.origin.y + popup.size.height <= px(1080.));
        if left {
            assert!(popup.right() < owner.left());
        } else {
            assert!(popup.left() > owner.right());
        }
    }
    let overlay = hover_popup_bounds(
        display,
        display,
        HoverPreviewPosition::Auto,
        None,
        false,
        px(500.),
    )
    .unwrap();
    assert!(overlay.left() >= display.left() && overlay.right() <= display.right());
    assert!(overlay.top() >= display.top() && overlay.bottom() <= display.bottom());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn follow_cursor_flips_and_clamps_within_work_area() {
    // 负坐标副屏：先放光标右下，放不下翻左，越界夹回工作区。
    let work = (-1920, 0, 0, 1040);
    assert_eq!(
        placement(
            WindowPositionPreference::FollowCursor,
            (-1000, 200),
            work,
            500,
            600
        ),
        (-988, 212)
    );
    assert_eq!(
        placement(
            WindowPositionPreference::FollowCursor,
            (-100, 900),
            work,
            500,
            600
        ),
        (-612, 440)
    );
    assert_eq!(
        placement(
            WindowPositionPreference::FollowCursor,
            (-1910, 10),
            work,
            2200,
            1200
        ),
        (-1920, 0)
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn screen_center_uses_selected_monitor_work_area() {
    // 居中取光标所在显示器的工作区，不是主屏的。
    assert_eq!(
        placement(
            WindowPositionPreference::ScreenCenter,
            (2500, 500),
            (1920, 40, 3840, 1080),
            560,
            760
        ),
        (2600, 180)
    );
}
