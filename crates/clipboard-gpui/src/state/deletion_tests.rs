use super::*;
use clipboard_core::History;

fn history_rows() -> (tempfile::TempDir, History, Vec<ClipboardItem>) {
    let directory = tempfile::tempdir().unwrap();
    let history = History::open(directory.path().join("clipboard.db")).unwrap();
    let first = history.capture("first").unwrap().unwrap();
    let second = history.capture("second").unwrap().unwrap();
    let rows = vec![history.item(first).unwrap(), history.item(second).unwrap()];
    (directory, history, rows)
}

#[test]
fn committed_deletion_updates_rows_selection_and_total_without_finishing_query() {
    let (_directory, history, rows) = history_rows();
    let deleted = rows[0].id;
    let remaining = rows[1].id;
    let mut state = HistoryState::default();
    state.apply(rows, 12, 0);
    state.loading = true; // A larger page is still in flight.
    history.delete(deleted).unwrap();

    assert!(state.apply_deleted(deleted));
    assert_eq!(
        state.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        [remaining]
    );
    assert_eq!(state.selected, Some(remaining));
    assert_eq!(state.total, 11);
    assert_eq!(state.generation, 0);
    assert!(state.loading);
    assert!(!state.apply_deleted(deleted));
    assert_eq!(state.total, 11);

    history.delete(remaining).unwrap();
    assert!(state.apply_deleted(remaining));
    assert_eq!(state.selected, None);
    assert!(state.items.is_empty());
    assert_eq!(state.total, 10); // Unloaded rows still count.
}

#[test]
fn snapshot_before_acknowledgement_does_not_decrement_total_twice() {
    let (_directory, history, rows) = history_rows();
    let deleted = rows[0].id;
    let remaining = rows[1].clone();
    let mut state = HistoryState::default();
    state.apply(rows, 2, 0);
    history.delete(deleted).unwrap();
    state.apply(vec![remaining.clone()], 1, 0);

    assert!(!state.apply_deleted(deleted));
    assert_eq!(state.total, 1);
    assert_eq!(state.selected, Some(remaining.id));
    assert_eq!(state.items[0].id, remaining.id);
}

#[test]
fn deletion_acknowledgement_respects_changed_filter_and_rejects_old_snapshot() {
    let (_directory, history, rows) = history_rows();
    let deleted = rows[0].id;
    let remaining = rows[1].clone();
    history.toggle_favorite(remaining.id).unwrap();
    let mut state = HistoryState::default();
    state.apply(rows.clone(), 2, 0);
    state.set_favorite_filter(true);
    history.delete(deleted).unwrap();

    assert!(!state.apply_deleted(deleted));
    assert!(state.loading);
    assert_eq!(state.total, 0);
    assert!(state.apply(vec![history.item(remaining.id).unwrap()], 1, 1));
    assert!(!state.apply_deleted(deleted));
    assert_eq!(state.total, 1);
    assert_eq!(state.selected, Some(remaining.id));
    assert!(!state.apply(rows, 2, 0));
    assert_eq!(state.items.len(), 1);

    // Switching to a filter that still contains the deleted ID must remove it.
    state.set_filter(HistoryFilter::All);
    state.apply(vec![remaining.clone()], 1, 2);
    history.delete(remaining.id).unwrap();
    assert!(state.apply_deleted(remaining.id));
    assert_eq!(state.total, 0);
}

#[test]
fn deferred_drag_snapshot_cannot_restore_a_committed_deletion() {
    let (_directory, history, rows) = history_rows();
    let deleted = rows[0].id;
    let remaining = rows[1].id;
    let mut state = HistoryState::default();
    state.apply(rows.clone(), 2, 0);
    let mut deferred = rows;
    let mut deferred_total = 2;
    history.delete(deleted).unwrap();

    assert!(remove_deleted_item(
        &mut deferred,
        &mut deferred_total,
        deleted
    ));
    assert!(!remove_deleted_item(
        &mut deferred,
        &mut deferred_total,
        deleted
    ));
    assert_eq!(state.items.len(), 2); // The active drag's rows remain frozen.
    assert!(state.apply(deferred, deferred_total, 0));
    assert!(!state.apply_deleted(deleted));
    assert_eq!(state.total, 1);
    assert_eq!(state.selected, Some(remaining));
    assert_eq!(state.items[0].id, remaining);
}
