use super::*;

fn paging_item(id: i64) -> clipboard_core::database::ClipboardItem {
    clipboard_core::database::ClipboardItem {
        id,
        content_type: "text".into(),
        text_content: None,
        html_content: None,
        rtf_content: None,
        image_path: None,
        file_paths: None,
        file_payload: None,
        content_hash: format!("paging-{id}"),
        semantic_hash: format!("paging-{id}"),
        preview: None,
        byte_size: 0,
        image_width: None,
        image_height: None,
        is_pinned: false,
        is_favorite_pinned: false,
        is_favorite: false,
        favorite_order: 0,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        access_count: 0,
        last_accessed_at: None,
        char_count: None,
        source_app_name: None,
        source_app_icon: None,
        note: None,
        group_id: None,
        formats: Vec::new(),
    }
}

#[test]
fn continuation_pages_merge_while_structural_snapshots_drop_other_slots() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1), paging_item(2)], 6, 0, false, 0);
    // A deeper page merges: slots outside both pages survive.
    state.apply(vec![paging_item(5), paging_item(6)], 6, 4, false, 0);
    assert_eq!(state.items.len(), 6);
    assert!(state.items[0].is_some());
    assert!(state.items[2].is_none());
    assert_eq!(state.items[4].as_ref().unwrap().id, 5);
    // A structural refresh discards everything outside its own page: captures
    // shift positions, so the old slot mapping is untrustworthy.
    state.apply(vec![paging_item(7)], 6, 0, true, 0);
    assert_eq!(state.items.len(), 6);
    assert_eq!(state.items[0].as_ref().unwrap().id, 7);
    assert!(state.items.iter().skip(1).all(Option::is_none));
}

#[test]
fn first_reply_of_a_new_view_replaces_the_previous_views_slots() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1), paging_item(2)], 4, 0, false, 0);
    // The same view paging deeper merges into the surviving slots.
    state.apply(vec![paging_item(3)], 4, 2, false, 0);
    assert!(state.items.iter().flatten().any(|item| item.id == 1));
    super::settle(state.set_group(Some(9)));
    // The new view's first page is non-structural yet must not merge: row zero
    // of the old view has no relationship with row zero of the new one.
    state.apply(vec![paging_item(8)], 4, 0, false, state.generation);
    assert!(!state.items.iter().flatten().any(|item| item.id == 1));
    assert!(!state.items.iter().flatten().any(|item| item.id == 3));
    assert!(state.items.iter().skip(1).all(Option::is_none));
}

#[test]
fn resize_grows_slots_when_the_total_increases_between_pages() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1)], 2, 0, false, 0);
    assert_eq!(state.items.len(), 2);
    // A capture arrived before the answer: the total grows and loaded rows keep their slots.
    state.apply(vec![paging_item(2)], 4, 1, false, 0);
    assert_eq!(state.items.len(), 4);
    assert_eq!(state.items[0].as_ref().unwrap().id, 1);
    assert_eq!(state.items[1].as_ref().unwrap().id, 2);
}

#[test]
fn selection_drops_into_unloaded_rows_without_stalling_keyboard_scroll() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1)], 4, 0, false, 0);
    assert_eq!(state.select_relative(1), Some(1));
    // Row one is not paged in: scrolling reaches it, the selection waits for data.
    assert_eq!(state.selected, None);
    assert_eq!(state.select_relative(1), Some(2));
    // Once the page arrives the next keypress selects the row as usual.
    state.apply(vec![paging_item(2)], 4, 2, false, 0);
    assert_eq!(state.selected, Some(2));
}

#[test]
fn jumping_across_history_bounds_metadata_and_rejects_departed_pages() {
    use clipboard_core::HISTORY_PAGE_ROWS as PAGE;

    let mut state = HistoryState::default();
    let total = 60_000;
    for page in (0..8).chain([56, 110, 198]) {
        let start = page * PAGE;
        state.items.watch(start..start + PAGE);
        let rows = (start..start + PAGE)
            .map(|i| paging_item(i as i64))
            .collect();
        assert!(state.apply(rows, total, start, false, 0));
        // In addition to the bounded scrolling window, All retains page zero.
        assert!(state.items.loaded().filter(|(i, _)| *i >= PAGE).count() <= 4 * PAGE);
        assert!(state.items.metadata_capacity() <= 8 * PAGE);
        assert_eq!(state.item_index(start as i64), Some(start));
    }

    let last = 198 * PAGE;
    assert_eq!(state.items.len(), total as usize);
    assert_eq!(state.items[0].as_ref().unwrap().id, 0);
    assert!(state.items[PAGE].is_none());
    assert_eq!(state.items.known_ids().next(), Some(0));
    assert!(!state.apply(vec![paging_item(-1)], 1, 0, false, 0));
    assert_eq!(state.item_index(last as i64), Some(last));
    assert_eq!(state.total, total);

    // Marks and selection use IDs rather than retaining the marked metadata.
    let mut marks = IndexSet::new();
    assert!(toggle_batch_mark(&mut marks, &state.items, None, 0));
    assert!(toggle_batch_mark(
        &mut marks,
        &state.items,
        None,
        last as i64
    ));
    assert!(marks.iter().copied().eq([0, last as i64]));
    assert!(state.apply_deleted(0));
    assert_eq!(state.item_index(last as i64), Some(last - 1));

    state.items.watch(0..PAGE);
    assert!(state.apply(vec![paging_item(1)], total - 1, 0, false, 0));
    assert_eq!(state.item_index(1), Some(0));
    assert!(state.items[last - 1].is_none());
}

#[test]
fn favorites_return_restores_all_before_and_after_empty_or_nonempty_replies() {
    for favorite_ids in [None, Some(vec![]), Some(vec![2])] {
        let mut state = HistoryState::default();
        state.apply(vec![paging_item(1), paging_item(2)], 2, 0, false, 0);
        let favorites = state.set_favorite_filter(true).unwrap().generation();
        assert_eq!(state.items.id_at(0), Some(1)); // Old rows remain drawable.
        if let Some(ids) = favorite_ids {
            let total = ids.len() as i64;
            state.apply(
                ids.into_iter().map(paging_item).collect(),
                total,
                0,
                false,
                favorites,
            );
            assert_eq!(state.total, total);
        }
        let all = state
            .set_category(ContentCategory::All)
            .unwrap()
            .generation();
        assert!(all > favorites);
        assert!(!state.loading);
        assert_eq!(state.total, 2);
        assert_eq!(state.selected, Some(1));
        assert_eq!(state.items[0].as_ref().unwrap().id, 1);
        assert!(!state.apply(vec![], 0, 0, false, favorites));
        assert!(!state.fail_query(favorites));
        // Another round trip before worker scope is synchronized is also instant.
        let favorites_again = state.set_favorite_filter(true).unwrap().generation();
        let latest = state
            .set_category(ContentCategory::All)
            .unwrap()
            .generation();
        assert!(latest > favorites_again && favorites_again > all);
        assert!(!state.loading);
        assert!(!state.can_page());
        assert!(!state.apply(vec![], 0, 0, false, all));
        assert!(state.apply(vec![paging_item(3)], 1, 0, false, latest));
        assert!(state.can_page());
        assert_eq!(state.items.known_ids().collect::<Vec<_>>(), [3]);
    }
}

#[test]
fn deep_all_return_draws_first_page_without_fetching_it_again() {
    use clipboard_core::HISTORY_PAGE_ROWS as PAGE;
    let mut state = HistoryState::default();
    let total = (32 * PAGE) as i64;
    state.apply(
        (0..PAGE as i64).map(paging_item).collect(),
        total,
        0,
        false,
        0,
    );
    let deep = 24 * PAGE;
    state.items.watch(deep..deep + PAGE);
    state.apply(vec![paging_item(-1)], total, deep, false, 0);
    let fav = state.set_favorite_filter(true).unwrap().generation();
    state.apply(vec![], 0, 0, false, fav);
    super::settle(state.set_favorite_filter(false));
    assert!(!state.loading);
    assert_eq!(state.items.loaded_range(0..PAGE).count(), PAGE);
    assert!(state.items.wants_page(0));
    assert_eq!(state.selected, Some(0));
}

#[test]
fn parked_all_never_crosses_search_group_category_or_reset() {
    for change in 0..5 {
        let mut state = HistoryState::default();
        state.apply(vec![paging_item(1)], 1, 0, false, 0);
        let fav = state.set_favorite_filter(true).unwrap().generation();
        state.apply(vec![], 0, 0, false, fav);
        match change {
            0 => super::settle(state.begin_search()),
            1 => super::settle(state.set_group(Some(5))),
            2 => super::settle(state.set_category(ContentCategory::Text)),
            3 => super::settle(state.reset_filters()),
            _ => state.invalidate_all_view(),
        }
        assert!(state.parked_all_items().is_none());
        super::settle(state.set_category(ContentCategory::All));
        assert!(state.loading);
        assert!(state.items.is_empty());
    }
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1)], 1, 0, false, 0);
    state.invalidate_all_view();
    super::settle(state.set_favorite_filter(true));
    assert!(state.parked_all_items().is_none());
}

#[test]
fn favorite_mutation_invalidates_all_until_its_matching_companion_page() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1)], 1, 0, false, 0);
    let fav = state.set_favorite_filter(true).unwrap().generation();
    state.apply(vec![], 0, 0, false, fav);
    assert!(state.parked_all_items().is_some());
    state.apply(vec![paging_item(2)], 1, 0, true, fav);
    assert!(state.parked_all_items().is_none());
    state.remember_all_page(vec![paging_item(4)], 1, fav - 1);
    assert!(state.parked_all_items().is_none());
    state.remember_all_page(vec![paging_item(3), paging_item(2)], 2, fav);
    assert_eq!(state.items.id_at(0), Some(2));
    let all = state.set_favorite_filter(false).unwrap().generation();
    assert!(!state.loading);
    assert_eq!(state.items.id_at(0), Some(3));
    state.remember_all_page(vec![], 0, all);
    assert!(state.parked_all_items().is_none());
}

#[test]
fn deletion_in_parked_all_is_never_resurrected() {
    for delete_before_favorites in [true, false] {
        let mut state = HistoryState::default();
        state.apply(vec![paging_item(1), paging_item(2)], 2, 0, false, 0);
        let fav = state.set_favorite_filter(true).unwrap().generation();
        if delete_before_favorites {
            assert!(state.apply_deleted(1));
        }
        state.apply(vec![], 0, 0, false, fav);
        if !delete_before_favorites {
            assert!(!state.apply_deleted(1)); // No displayed Favorites changed.
        }
        super::settle(state.set_favorite_filter(false));
        assert!(!state.loading);
        assert_eq!(state.total, 1);
        assert_eq!(state.items[0].as_ref().unwrap().id, 2);
    }
}

#[test]
fn deletion_at_top_shifts_the_next_resident_page_without_copying_or_holes() {
    use clipboard_core::HISTORY_PAGE_ROWS as PAGE;
    let mut state = HistoryState::default();
    let ids = 0..=PAGE as i64;
    let total = ids.clone().count() as i64;
    state.apply(ids.map(paging_item).collect(), total, 0, false, 0);
    assert!(state.apply_deleted(0));
    assert_eq!(state.items.loaded_range(0..PAGE).count(), PAGE);
    assert_eq!(state.items[PAGE - 1].as_ref().unwrap().id, PAGE as i64);
    assert_eq!(state.items.iter().count(), PAGE);
    assert_eq!(state.items.loaded().count(), PAGE);
}

#[test]
fn unknown_deleted_position_invalidates_sparse_parked_all() {
    let mut state = HistoryState::default();
    state.apply(vec![paging_item(1)], 10, 0, false, 0);
    let fav = state.set_favorite_filter(true).unwrap().generation();
    state.apply(vec![], 0, 0, false, fav);
    assert!(!state.apply_deleted(2));
    assert!(state.parked_all_items().is_none());
    super::settle(state.set_favorite_filter(false));
    assert!(state.loading);
}
