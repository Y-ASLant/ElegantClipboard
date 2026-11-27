use super::{DragMotionState, DragPlacement, DropTarget, Pixels, px, visual};

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
        let item_at = |index| items.get(index).copied();
        let mut motion = DragMotionState::default();
        let target = motion
            .hover_target(source, item_at, &offsets, px(first_y), None)
            .unwrap();
        assert_eq!(target.after, expected_after);
        motion.to = DragPlacement::new(source, 1 - source, target.after);
        let repeated =
            motion.hover_target(source, item_at, &offsets, px(first_y + 1.), Some(target));
        assert_eq!(
            repeated.map(|target| (target.id, target.after)),
            Some((target.id, target.after))
        );
        motion.hover_target(source, item_at, &offsets, px(leave_y), Some(target));
        let reverse = motion
            .hover_target(source, item_at, &offsets, px(return_y), Some(target))
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
    let item_at = |index| items.get(index).copied();
    let target = motion
        .hover_target(1, item_at, &offsets, px(220.), None)
        .unwrap();
    motion.to = DragPlacement::new(1, 2, target.after);
    let blocked = motion
        .hover_target(1, item_at, &offsets, px(20.), Some(target))
        .unwrap();
    assert_eq!((blocked.id, blocked.after), (target.id, target.after));
    assert_eq!(motion.hovered, None);
}

#[test]
fn hover_uses_painted_progress_and_ignores_the_invisible_source() {
    let offsets = [px(0.), px(100.), px(200.), px(300.)];
    let items = [(10, false), (20, false), (30, false)];
    let item_at = |index| items.get(index).copied();
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
            .hover_target(0, item_at, &offsets, px(225.), None)
            .unwrap()
            .id,
        30
    );
    assert_eq!(
        motion
            .hover_target(0, item_at, &offsets, px(125.), None)
            .unwrap()
            .id,
        20
    );
}

#[test]
fn hover_boundaries_follow_the_last_painted_frame() {
    let offsets = [px(0.), px(100.), px(200.), px(300.)];
    let items = [(10, false), (20, false), (30, false)];
    let item_at = |index| items.get(index).copied();
    let mut motion = DragMotionState::default();
    motion.transition(
        DragPlacement::new(0, 2, true),
        &offsets,
        std::time::Instant::now(),
    );
    // A transition has been requested but no frame has moved the cards yet.
    assert!(
        motion
            .hover_target(0, item_at, &offsets, px(99.), None)
            .is_none()
    );
    assert_eq!(
        motion
            .hover_target(0, item_at, &offsets, px(100.), None)
            .unwrap()
            .id,
        20
    );
    motion.hovered = None;
    assert_eq!(
        motion
            .hover_target(0, item_at, &offsets, px(200.), None)
            .unwrap()
            .id,
        30
    );
    // After a final frame (including reduced motion), use final half-open bounds.
    motion.progress.set(1.);
    motion.hovered = None;
    assert_eq!(
        motion
            .hover_target(0, item_at, &offsets, px(0.), None)
            .unwrap()
            .id,
        20
    );
    assert_eq!(
        motion
            .hover_target(0, item_at, &offsets, px(100.), None)
            .unwrap()
            .id,
        30
    );
    assert!(
        motion
            .hover_target(0, item_at, &offsets, px(200.), None)
            .is_none()
    );
}

#[test]
fn a_stationary_pointer_does_not_reverse_the_same_moving_target() {
    let offsets = [px(0.), px(96.), px(220.)];
    let items = [(10, false), (20, false)];
    let item_at = |index| items.get(index).copied();
    let mut motion = DragMotionState::default();
    let target = motion
        .hover_target(0, item_at, &offsets, px(110.), None)
        .unwrap();
    motion.transition(
        DragPlacement::new(0, 1, true),
        &offsets,
        std::time::Instant::now(),
    );
    for progress in [0., 0.25, 0.5, 0.75, 1.] {
        motion.progress.set(progress);
        let current = motion
            .hover_target(0, item_at, &offsets, px(110.), Some(target))
            .unwrap();
        assert_eq!((current.id, current.after), (target.id, target.after));
    }
}

#[test]
fn filtered_domain_ids_drive_variable_height_placement() {
    let ids = [None, Some(71), None, Some(9), Some(104)];
    let index_of = |id| ids.iter().position(|&row| row == Some(id));
    let offsets = [px(0.), px(80.), px(120.), px(200.), px(300.), px(360.)];
    let mut motion = DragMotionState::default();
    motion.update(
        Some((
            71,
            DropTarget {
                id: 104,
                after: true,
            },
        )),
        index_of,
        &offsets,
    );
    assert!(motion.to == DragPlacement::new(1, 4, true));
    assert_eq!(motion.offsets(1, &offsets), (0., 240.));
    assert_eq!(motion.offsets(2, &offsets), (0., -40.));
    motion.update(
        Some((
            9,
            DropTarget {
                id: 71,
                after: true,
            },
        )),
        index_of,
        &offsets,
    );
    // The unloaded row between the two IDs is a real insertion slot.
    assert!(motion.to == DragPlacement::new(3, 1, true));
    motion.update(
        Some((
            104,
            DropTarget {
                id: 999,
                after: false,
            },
        )),
        index_of,
        &offsets,
    );
    assert!(motion.to.is_none());
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

fn scanned_target(
    motion: &DragMotionState,
    source: usize,
    items: &[(i64, bool)],
    offsets: &[Pixels],
    pointer: Pixels,
    previous: Option<DropTarget>,
) -> Option<(i64, bool)> {
    let progress = motion.started.map_or(1., |_| motion.progress.get());
    let top_at = |index| {
        let (from, to) = motion.offsets(index, offsets);
        offsets[index] + px(from + (to - from) * progress)
    };
    let source_top = top_at(source);
    for (index, &(id, pinned)) in items.iter().enumerate() {
        if index == source {
            continue;
        }
        let top = top_at(index);
        let bottom = top + offsets[index + 1] - offsets[index];
        if pointer >= top && pointer < bottom {
            if pinned == items[source].1 {
                return Some((id, top > source_top));
            }
            break;
        }
    }
    previous.map(|target| (target.id, target.after))
}

#[test]
fn interrupted_mixed_height_hit_testing_matches_first_painted_overlap() {
    let offsets = [
        px(0.),
        px(24.),
        px(240.),
        px(280.),
        px(344.),
        px(500.),
        px(516.),
    ];
    let items = [
        (10, true),
        (20, true),
        (30, false),
        (40, false),
        (50, false),
        (60, false),
    ];
    let start = std::time::Instant::now();
    for source in 0..items.len() {
        let mut motion = DragMotionState::default();
        let previous = Some(DropTarget {
            id: 999,
            after: false,
        });
        for target in [5, 0, 4, 1, 3, 2] {
            motion.transition(
                DragPlacement::new(source, target, target > source),
                &offsets,
                start,
            );
            for progress in [0., 0.25, 0.5, 0.75, 1.] {
                motion.progress.set(progress);
                for index in 0..items.len() {
                    if index != source {
                        let (from, to) = motion.offsets(index, &offsets);
                        let height = f32::from(offsets[source + 1] - offsets[source]);
                        assert!(from.abs() <= height && to.abs() <= height);
                    }
                }
                for half_pixel in -2..1036 {
                    let pointer = px(half_pixel as f32 / 2.);
                    let expected =
                        scanned_target(&motion, source, &items, &offsets, pointer, previous);
                    motion.hovered = None;
                    let actual = motion.hover_target(
                        source,
                        |index| items.get(index).copied(),
                        &offsets,
                        pointer,
                        previous,
                    );
                    assert_eq!(actual.map(|target| (target.id, target.after)), expected);
                }
            }
            // Interrupt before the current placement reaches its endpoint.
            motion.progress.set(0.5);
        }
    }
}

#[test]
fn new_drag_can_hit_the_previous_drag_source_before_it_settles() {
    let offsets = [px(0.), px(20.), px(120.), px(220.), px(320.), px(420.)];
    let items = [
        (10, false),
        (20, false),
        (30, false),
        (40, false),
        (50, false),
    ];
    let start = std::time::Instant::now();
    let mut motion = DragMotionState::default();
    motion.transition(DragPlacement::new(4, 0, false), &offsets, start);
    motion.progress.set(1.);
    // Source 50 is at 0..100: much farther than the new source's 20px height.
    assert_eq!(
        motion
            .hover_target(
                0,
                |index| items.get(index).copied(),
                &offsets,
                px(10.),
                None
            )
            .unwrap()
            .id,
        50
    );
    motion.transition(None, &offsets, start);
    motion.progress.set(0.25);
    motion.hovered = None;
    // While returning, row 50 is at 80..180 and overlaps row 20 at 95..195.
    // At 90 only the old source is hit; at 100 earlier model row 20 wins.
    for (pointer, expected) in [(90., 50), (100., 20)] {
        assert_eq!(
            motion
                .hover_target(
                    0,
                    |index| items.get(index).copied(),
                    &offsets,
                    px(pointer),
                    None
                )
                .unwrap()
                .id,
            expected
        );
        motion.hovered = None;
    }
}
