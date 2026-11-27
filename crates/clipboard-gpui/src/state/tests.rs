use super::*;
use clipboard_core::{History, PAGE_SIZE};

#[test]
fn missing_icon_falls_back_to_the_source_name() {
    assert_eq!(
        source_app_parts(SourceAppDisplay::Icon, false),
        (true, false)
    );
}

#[test]
fn card_time_matches_absolute_and_relative_display_choices() {
    let now = NaiveDateTime::parse_from_str("2026-09-25 00:10:00", "%Y-%m-%d %H:%M:%S").unwrap();
    assert_eq!(
        format_card_time(
            "2026-09-25 00:05:00",
            TimeFormat::Absolute,
            LanguagePreference::Chinese,
            now
        ),
        "今天 00:05"
    );
    assert_eq!(
        format_card_time(
            "2026-09-24 23:59:00",
            TimeFormat::Absolute,
            LanguagePreference::English,
            now
        ),
        "Yesterday 23:59"
    );
    assert_eq!(
        format_card_time(
            "2026-09-22 07:30:00",
            TimeFormat::Absolute,
            LanguagePreference::Chinese,
            now
        ),
        "09-22 07:30"
    );
    assert_eq!(
        format_card_time(
            "2026-09-25 00:09:30",
            TimeFormat::Relative,
            LanguagePreference::Chinese,
            now
        ),
        "刚刚"
    );
    assert_eq!(
        format_card_time(
            "2026-09-25 00:05:00",
            TimeFormat::Relative,
            LanguagePreference::English,
            now
        ),
        "5 min ago"
    );
    assert_eq!(
        format_card_time(
            "invalid",
            TimeFormat::Relative,
            LanguagePreference::English,
            now
        ),
        "invalid"
    );
}

#[test]
fn sequence_selection_keeps_click_order_and_appends_shift_ranges() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("history.db"))?;
    for text in ["alpha", "beta", "gamma", "delta"] {
        history.capture(text)?;
    }
    let rows = history.list("", PAGE_SIZE, false)?;
    let mut items = HistoryItems::default();
    items.merge(rows.clone(), rows.len(), 0, true, false);
    let [first, second, third, last] = rows.as_slice() else {
        panic!("expected four history rows");
    };
    let mut selected = IndexSet::new();

    for id in [last.id, first.id, second.id] {
        assert!(toggle_batch_mark(&mut selected, &items, None, id));
    }
    assert!(selected.iter().copied().eq([last.id, first.id, second.id]));

    // Rechecking the first mark puts it last without moving the other two.
    toggle_batch_mark(&mut selected, &items, None, last.id);
    toggle_batch_mark(&mut selected, &items, None, last.id);
    assert!(selected.iter().copied().eq([first.id, second.id, last.id]));

    selected.clear();
    toggle_batch_mark(&mut selected, &items, None, last.id);
    toggle_batch_mark(&mut selected, &items, None, second.id);
    toggle_batch_mark(&mut selected, &items, Some(last.id), first.id);
    assert!(
        selected
            .iter()
            .copied()
            .eq([last.id, second.id, third.id, first.id])
    );

    selected.clear();
    toggle_batch_mark(&mut selected, &items, Some(i64::MAX), second.id);
    assert!(!toggle_batch_mark(
        &mut selected,
        &items,
        Some(second.id),
        i64::MAX
    ));
    toggle_batch_mark(&mut selected, &items, Some(second.id), last.id);
    assert!(selected.iter().copied().eq([second.id, third.id, last.id]));
    Ok(())
}

#[test]
fn committed_reorder_animates_each_row_from_its_old_pixel_position() {
    let offsets = reorder_pixel_offsets(&[4, 3, 2, 1], &[(3, 124.), (2, 88.), (4, 96.), (1, 148.)]);
    assert_eq!(offsets, HashMap::from([(4, -212.), (3, 96.), (2, 96.)]));
    assert!(reorder_pixel_offsets(&[1, 2], &[(3, 96.), (2, 88.)]).is_empty());
    assert!(reorder_pixel_offsets(&[1, 2], &[(1, 96.)]).is_empty());
    assert!(reorder_pixel_offsets(&[1, 2], &[(1, 96.), (1, 88.)]).is_empty());
}

#[test]
fn drag_scroll_stops_at_loaded_list_edges() {
    assert_eq!(next_drag_scroll_index(4, 10, -1), 3);
    assert_eq!(next_drag_scroll_index(0, 10, -1), 0);
    assert_eq!(next_drag_scroll_index(4, 10, 1), 5);
    assert_eq!(next_drag_scroll_index(9, 10, 1), 9);
    assert_eq!(next_drag_scroll_index(4, 10, 0), 4);
    assert_eq!(next_drag_scroll_index(0, 0, 1), 0);
    assert_eq!(drag_edge_target_index(4, 10, 2, -1), Some(4));
    assert_eq!(drag_edge_target_index(4, 10, 2, 1), Some(6));
    assert_eq!(drag_edge_target_index(8, 10, 2, 1), Some(9));
    assert_eq!(drag_edge_target_index(0, 0, 2, 1), None);
    assert_eq!(drag_edge_target_index(4, 10, 2, 0), None);
}

#[test]
fn unchanged_scope_keeps_the_in_flight_generation() {
    let mut state = HistoryState::default();
    assert!(state.set_category(ContentCategory::All).is_none());
    assert_eq!(state.generation, 0);
    assert!(state.loading);

    assert!(state.set_group(Some(9)).is_some());
    let generation = state.generation;
    assert!(state.set_group(Some(9)).is_none());
    assert_eq!(state.generation, generation);
    assert!(state.loading);
    assert!(!state.apply(vec![], 0, 0, true, generation - 1));
    assert!(state.apply(vec![], 0, 0, true, generation));
    assert!(!state.loading);

    assert!(state.set_category(ContentCategory::Text).is_some());
    let generation = state.generation;
    assert!(state.set_category(ContentCategory::Text).is_none());
    assert_eq!(state.generation, generation);
    assert!(state.loading);
    assert!(state.apply(vec![], 0, 0, true, generation));

    assert!(state.set_favorite_filter(true).is_some());
    assert!(state.favorite_only());
    assert_eq!(state.category(), ContentCategory::All);
    let generation = state.generation;
    assert!(state.set_favorite_filter(true).is_none());
    assert_eq!(state.generation, generation);
    assert!(state.loading);
    assert!(state.set_category(ContentCategory::All).is_some());
    assert!(!state.favorite_only());
    assert!(state.generation > generation);
}

#[test]
fn filter_change_rejects_previous_results() {
    let mut state = HistoryState {
        selected: Some(42),
        ..Default::default()
    };
    let old = state.generation;
    super::settle(state.set_favorite_filter(true));
    assert!(state.favorite_only());
    assert_eq!(state.selected, None);
    assert!(!state.apply(vec![], 100, 0, true, old));
    super::settle(state.begin_search());
    assert!(state.favorite_only());
    assert!(state.apply(vec![], 0, 0, true, state.generation));
    super::settle(state.set_favorite_filter(false));
    assert!(!state.favorite_only());
    assert!(state.loading);
    let old = state.generation;
    super::settle(state.set_group(Some(7)));
    assert_eq!(state.group_id(), Some(7));
    assert!(!state.apply(vec![], 1, 0, true, old));
    assert!(!state.fail_query(old));
    assert!(state.loading);
    assert!(state.fail_query(state.generation));
    assert!(!state.loading);
    super::settle(state.set_group(None));
    assert_eq!(state.group_id(), None);
    let old = state.generation;
    super::settle(state.set_category(ContentCategory::Other));
    assert_eq!(state.category(), ContentCategory::Other);
    assert!(!state.favorite_only());
    assert!(!state.apply(vec![], 0, 0, true, old));
    super::settle(state.set_favorite_filter(true));
    assert_eq!(state.category(), ContentCategory::All);
    assert!(state.favorite_only());
    super::settle(state.set_group(Some(7)));
    assert!(state.favorite_only());
    assert_eq!(state.category(), ContentCategory::All);
    let old = state.generation;
    super::settle(state.reset_filters());
    assert_eq!(state.group_id(), None);
    assert!(!state.favorite_only());
    assert_eq!(state.category(), ContentCategory::All);
    assert!(!state.apply(vec![], 100, 0, true, old));
}

#[test]
fn category_and_favorite_filters_preserve_group_selection() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("history.db"))?;
    let group = history.create_group("Work")?;
    let id = history.capture("hello")?.unwrap();
    history.move_to_group(id, None, Some(group.id))?;
    let items = history.list_in_group("", PAGE_SIZE, false, Some(group.id))?;
    let mut state = HistoryState::default();

    super::settle(state.set_group(Some(group.id)));
    assert!(state.apply(items, 1, 0, true, state.generation));
    assert_eq!(state.select_index(0), Some(0));
    assert_eq!(state.selected, Some(id));
    assert_eq!(state.group_id(), Some(group.id));

    for category in [ContentCategory::Text, ContentCategory::Other] {
        super::settle(state.set_category(category));
        assert_eq!(state.category(), category);
        assert_eq!(state.group_id(), Some(group.id));
        super::settle(state.set_category(ContentCategory::All));
        assert_eq!(state.category(), ContentCategory::All);
        assert_eq!(state.group_id(), Some(group.id));
    }

    super::settle(state.set_favorite_filter(true));
    assert!(state.favorite_only());
    assert_eq!(state.group_id(), Some(group.id));
    super::settle(state.set_category(ContentCategory::All));
    assert!(!state.favorite_only());
    assert_eq!(state.group_id(), Some(group.id));
    Ok(())
}

#[test]
fn group_tabs_and_search_compose_without_leaking_rows_or_selection() -> anyhow::Result<()> {
    use clipboard_core::database::{ClipboardRepository, ContentType, Database, NewClipboardItem};

    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path().join("history.db"))?;
    let history = History::new(&db);
    let first = history.create_group("First")?.id;
    let second = history.create_group("Second")?.id;
    let mut ids = Vec::new();
    for (index, (group, kind, favorite, text)) in [
        (None, ContentType::Text, true, "needle default"),
        (Some(first), ContentType::Text, true, "needle first"),
        (Some(first), ContentType::Files, true, "needle files"),
        (Some(first), ContentType::Text, false, "unmatched"),
        (Some(second), ContentType::Text, true, "needle second"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = ClipboardRepository::new(&db).insert(NewClipboardItem {
            content_type: kind,
            text_content: Some(text.into()),
            content_hash: format!("filter-{index}"),
            semantic_hash: format!("filter-{index}"),
            ..Default::default()
        })?;
        if group.is_some() {
            history.move_to_group(id, None, group)?;
        }
        if favorite {
            history.toggle_favorite(id)?;
        }
        ids.push(id);
    }
    let query = |state: &mut HistoryState, search: &str, expected: &[i64]| -> anyhow::Result<()> {
        let rows = history.list_filtered_in_group(
            search,
            0,
            state.favorite_only(),
            state.group_id(),
            state.category(),
        )?;
        let count = history.count_filtered_in_group(
            search,
            state.favorite_only(),
            state.group_id(),
            state.category(),
        )?;
        let mut actual: Vec<_> = rows.iter().map(|item| item.id).collect();
        actual.sort_unstable();
        assert_eq!(actual, expected);
        assert_eq!(count, expected.len() as i64);
        assert!(state.apply(rows, count, 0, true, state.generation));
        assert!(state.selected.is_none_or(|id| expected.contains(&id)));
        Ok(())
    };
    let mut state = HistoryState::default();
    query(&mut state, "needle", &ids[..1])?;
    super::settle(state.set_category(ContentCategory::Text));
    super::settle(state.set_group(Some(first)));
    query(&mut state, "needle", &ids[1..2])?;
    let stale_rows = state.items.loaded().map(|(_, row)| row.clone()).collect();
    let stale_generation = state.generation;
    super::settle(state.set_group(Some(second)));
    assert_eq!(state.selected, None);
    // Rows of the previous group stay visible until the new snapshot swaps them.
    assert!(!state.items.is_empty());
    assert!(!state.apply(stale_rows, 1, 0, true, stale_generation));
    assert!(!state.fail_query(stale_generation));
    query(&mut state, "needle", &ids[4..5])?;
    super::settle(state.set_category(ContentCategory::Other));
    super::settle(state.set_group(Some(first)));
    query(&mut state, "needle", &ids[2..3])?;
    super::settle(state.set_favorite_filter(true));
    query(&mut state, "needle", &ids[1..3])?;
    super::settle(state.set_group(None));
    query(&mut state, "needle", &ids[..1])?;
    super::settle(state.set_group(Some(first)));
    super::settle(state.begin_search());
    query(&mut state, "files", &ids[2..3])?;
    super::settle(state.begin_search());
    query(&mut state, "unmatched", &[])?;
    super::settle(state.set_category(ContentCategory::Text));
    query(&mut state, "unmatched", &ids[3..4])?;
    super::settle(state.reset_filters());
    query(&mut state, "", &ids[..1])?;
    Ok(())
}

#[test]
fn preview_rejects_closed_switched_and_reopened_requests() {
    let mut preview = PreviewState::default();
    let first = preview.open(1);
    preview.close();
    assert!(!preview.apply(1, first, Ok(PreviewContent::Text("late".into()))));
    let second = preview.open(1);
    assert!(!preview.apply(1, first, Ok(PreviewContent::Text("old".into()))));
    assert!(preview.apply(1, second, Ok(PreviewContent::Text("full text".into()))));
    let third = preview.open(2);
    assert!(preview.result.is_none());
    assert!(!preview.apply(1, second, Ok(PreviewContent::Text("old selection".into()))));
    assert!(preview.apply(2, third, Err("记录已不存在".into())));
    assert!(preview.result.as_ref().unwrap().is_err());
}

#[test]
fn late_search_results_are_rejected_and_selection_follows_ids() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("history.db"))?;
    let a = history.capture("alpha")?.unwrap();
    let b = history.capture("beta")?.unwrap();
    let mut state = HistoryState::default();
    state.apply(history.list("", PAGE_SIZE, false)?, 2, 0, true, 0);
    state.selected = Some(a);
    history.capture("alpha")?;
    state.apply(history.list("", PAGE_SIZE, false)?, 2, 0, true, 0);
    assert_eq!(state.selected, Some(a));
    assert_eq!(state.select_relative(1), Some(1));
    assert_eq!(state.selected, Some(b));
    assert_eq!(state.select_relative(1), Some(1));
    assert_eq!(state.select_relative(-1), Some(0));
    assert_eq!(state.select_relative(-1), Some(0));
    assert_eq!(state.select_index(usize::MAX), Some(1));
    assert_eq!(state.selected, Some(b));
    assert_eq!(state.select_index(0), Some(0));
    assert_eq!(state.selected, Some(a));
    super::settle(state.begin_search());
    assert!(!state.apply(history.list("", PAGE_SIZE, false)?, 2, 0, true, 0));
    // A pending reload keeps the stale rows instead of blanking the list.
    assert!(!state.items.is_empty());
    assert!(state.apply(history.list("beta", PAGE_SIZE, false)?, 1, 0, true, 1));
    assert_eq!(state.selected, Some(b));
    history.delete(b)?;
    state.apply(vec![], 0, 0, true, 1);
    assert_eq!(state.select_relative(1), None);
    assert_eq!(state.select_index(0), None);
    assert_eq!(state.selected, None);
    Ok(())
}

#[test]
fn source_app_visible_name_keeps_detail_when_enabled() {
    assert_eq!(
        source_app_visible_name("chrome：https://example.com", true),
        "chrome：https://example.com"
    );
}

#[test]
fn source_app_visible_name_strips_detail_when_disabled() {
    assert_eq!(
        source_app_visible_name("chrome：https://example.com", false),
        "chrome"
    );
}

#[test]
fn source_app_visible_name_leaves_plain_name() {
    assert_eq!(source_app_visible_name("Notepad", false), "Notepad");
}
