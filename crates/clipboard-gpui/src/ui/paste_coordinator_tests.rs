use super::*;

fn request(command: &Command) -> u64 {
    match command {
        Command::Copy {
            purpose: CopyPurpose::Paste { request_id },
            ..
        } => *request_id,
        _ => panic!("expected a sequence paste copy"),
    }
}

#[test]
fn cancellation_and_reentry_reject_late_receipts_failures_and_delivery() {
    let mut state = PasteCoordinator::default();
    let old = state.begin_copy((1, 2));
    assert_eq!(state.receive(old), Some((1, 2)));
    assert_eq!(state.interrupt(None).request_id, Some(old));
    let current = state.begin_copy((3, 4));
    assert_eq!(state.receive(old), None);
    assert!(state.fail(old).is_none());
    assert_eq!(state.complete_delivery(old), None);
    assert!(state.pending());
    assert_eq!(state.receive(current), Some((3, 4)));
    assert_eq!(state.receive(current), None);
    assert!(state.delivering(current));
    state.complete_delivery(current);
    assert!(!state.pending());
}

#[test]
fn first_sequence_command_preserves_queue_and_advance_waits_for_delivery() {
    let mut state = PasteCoordinator::default();
    assert!(state.start_sequence(vec![10, 20], (1, 2)));
    let first = state.advance_sequence().unwrap();
    let first_request = request(&first);
    let interrupted = state.replace(&first).unwrap();
    assert!(!interrupted.sequence);
    assert_eq!(interrupted.request_id, None);
    assert_eq!(state.sequence_remaining(), Some(2));
    assert!(state.advance_sequence().is_none());
    assert_eq!(state.complete_delivery(first_request), None);
    assert!(state.pending());
    assert_eq!(state.receive(first_request), Some((1, 2)));
    assert!(state.advance_sequence().is_none());
    assert_eq!(state.complete_delivery(first_request), None);
    assert_eq!(state.sequence_remaining(), Some(1));
    let second = state.advance_sequence().unwrap();
    assert!(matches!(second, Command::Copy { id: 20, .. }));
    let second_request = request(&second);
    state.replace(&second);
    assert_eq!(state.receive(first_request), None);
    assert_eq!(state.receive(second_request), Some((1, 2)));
    assert_eq!(state.complete_delivery(first_request), None);
    assert_eq!(state.sequence_remaining(), Some(1));
    assert_eq!(state.complete_delivery(second_request), Some(2));
    assert_eq!(state.sequence_remaining(), None);
    assert_eq!(state.target(), None);
    assert!(state.advance_sequence().is_none());
}

#[test]
fn automatic_hide_and_target_activation_preserve_inflight_sequence() {
    let mut state = PasteCoordinator::default();
    state.set_target(Some((1, 2)));
    state.start_sequence(vec![10, 20], (1, 2));
    let command = state.advance_sequence().unwrap();
    let id = request(&command);
    state.replace(&command);
    state.receive(id);
    assert!(state.deactivate(Some((1, 2))).is_none());
    assert!(state.hide().is_none());
    assert_eq!(state.target(), None);
    assert!(state.delivering(id));
    state.complete_delivery(id);
    assert!(state.deactivate(Some((1, 2))).is_none());
    assert_eq!(state.sequence_remaining(), Some(1));
    let (interrupted, was_active) = state.deactivate(Some((3, 4))).unwrap();
    assert!(was_active);
    assert!(interrupted.sequence);
    assert!(state.advance_sequence().is_none());
}

#[test]
fn ordinary_delivery_and_reopening_preserve_only_new_target() {
    let mut state = PasteCoordinator::default();
    state.set_target(Some((1, 2)));
    let old = state.begin_copy((1, 2));
    state.receive(old);
    assert!(state.deactivate(Some((1, 2))).is_none());
    assert!(state.hide().is_none());
    state.set_target(Some((3, 4)));
    assert_eq!(state.show().request_id, Some(old));
    assert_eq!(state.target(), Some((3, 4)));
    assert_eq!(state.receive(old), None);
    let current = state.begin_copy((3, 4));
    state.receive(current);
    state.complete_delivery(current);
    assert_eq!(state.target(), None);
    assert!(!state.pending());
}

#[test]
fn quick_failure_and_late_resolution_cannot_consume_a_new_slot_request() {
    let mut state = PasteCoordinator::default();
    let old = state.begin_quick((1, 2));
    state.show();
    let current = state.begin_quick((3, 4));
    state.replace(&Command::ResolveQuickPaste {
        request_id: current,
        slot: 1,
        favorite: false,
        group_id: Some(9),
    });
    assert_eq!(state.resolve_quick(old), None);
    assert!(state.quick_pending());
    // Failure and success both consume only their own quick-resolution request.
    assert_eq!(state.resolve_quick(current), Some((3, 4)));
    assert!(!state.quick_pending());
    assert!(!state.pending());
    let retry = state.begin_quick((5, 6));
    assert_eq!(state.resolve_quick(current), None);
    let target = state.resolve_quick(retry).unwrap();
    let paste_request = state.begin_copy(target);
    assert_eq!(state.receive(paste_request), Some((5, 6)));
}

#[test]
fn merge_completion_and_failure_release_batch_and_request_ownership() {
    let mut state = PasteCoordinator::default();
    let old = state.begin_copy((1, 2));
    state.replace(&Command::Merge {
        ids: vec![10, 20],
        purpose: CopyPurpose::Paste { request_id: old },
    });
    state.mark_merge_started();
    let interrupted = state.fail(old).unwrap();
    assert!(interrupted.merge);
    assert_eq!(interrupted.request_id, Some(old));
    let current = state.begin_copy((3, 4));
    state.mark_merge_started();
    assert!(state.fail(old).is_none());
    assert!(state.merging());
    assert_eq!(state.receive(current), Some((3, 4)));
    state.finish_merge();
    assert!(!state.merging());
    assert!(state.delivering(current));
    state.complete_delivery(current);
    assert!(!state.pending());
}

#[test]
fn clipboard_replacement_cancels_sequence_and_quick_resolution() {
    let mut state = PasteCoordinator::default();
    state.start_sequence(vec![10, 20], (1, 2));
    let first = state.advance_sequence().unwrap();
    let id = request(&first);
    state.replace(&first);
    state.receive(id);
    let replacement = Command::Copy {
        id: 30,
        mode: CopyMode::Transform(clipboard_core::TextTransform::Trim),
        purpose: CopyPurpose::Copy,
    };
    let interrupted = state.replace(&replacement).unwrap();
    assert_eq!(interrupted.request_id, Some(id));
    assert!(interrupted.sequence);
    assert!(!state.delivering(id));
    assert_eq!(state.complete_delivery(id), None);
    let quick = state.begin_quick((3, 4));
    state.replace(&replacement);
    assert_eq!(state.resolve_quick(quick), None);
}

#[test]
fn sequence_copy_failure_cancels_queue_without_accepting_old_failure_on_retry() {
    let mut state = PasteCoordinator::default();
    state.start_sequence(vec![10, 20], (1, 2));
    let command = state.advance_sequence().unwrap();
    let old = request(&command);
    state.replace(&command);
    assert!(state.fail(old).unwrap().sequence);
    assert!(state.start_sequence(vec![30, 40], (3, 4)));
    let retry = state.advance_sequence().unwrap();
    let current = request(&retry);
    state.replace(&retry);
    assert!(state.fail(old).is_none());
    assert_eq!(state.sequence_remaining(), Some(2));
    assert_eq!(state.receive(current), Some((3, 4)));
}
