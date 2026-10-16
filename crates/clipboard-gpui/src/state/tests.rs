use super::*;
use clipboard_core::History;

#[test]
fn source_app_icon_mode_falls_back_to_name_when_icon_is_missing() {
    assert_eq!(source_app_parts(SourceAppDisplay::Both, true), (true, true));
    assert_eq!(
        source_app_parts(SourceAppDisplay::Name, true),
        (true, false)
    );
    assert_eq!(
        source_app_parts(SourceAppDisplay::Icon, true),
        (false, true)
    );
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
fn selection_range_uses_loaded_display_order_in_both_directions() {
    let ids = [14, 9, 7, 2];
    assert_eq!(selection_range_ids(&ids, 9, 2), Some(&ids[1..=3]));
    assert_eq!(selection_range_ids(&ids, 2, 9), Some(&ids[1..=3]));
    assert_eq!(selection_range_ids(&ids, 7, 7), Some(&ids[2..=2]));
    assert_eq!(selection_range_ids(&ids, 99, 7), None);
    assert_eq!(selection_range_ids(&ids, 9, 99), None);
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
fn filter_change_resets_paging_and_rejects_previous_results() {
    let mut state = HistoryState {
        limit: PAGE_SIZE * 3,
        selected: Some(42),
        ..Default::default()
    };
    let old = state.generation;
    state.set_favorite_filter(true);
    assert!(state.favorite_only());
    assert_eq!(state.limit, PAGE_SIZE);
    assert_eq!(state.selected, None);
    assert!(!state.apply(vec![], 100, old));
    state.begin_search();
    assert!(state.favorite_only());
    assert!(state.apply(vec![], 0, state.generation));
    state.set_favorite_filter(false);
    assert!(!state.favorite_only());
    assert!(state.loading);
    let old = state.generation;
    state.set_group(Some(7));
    assert_eq!(state.group_id(), Some(7));
    assert!(!state.apply(vec![], 1, old));
    assert!(!state.fail_query(old));
    assert!(state.loading);
    assert!(state.fail_query(state.generation));
    assert!(!state.loading);
    state.set_group(None);
    assert_eq!(state.group_id(), None);
    let old = state.generation;
    state.set_category(ContentCategory::Other);
    assert_eq!(state.category(), ContentCategory::Other);
    assert!(!state.favorite_only());
    assert!(!state.apply(vec![], 0, old));
    state.set_favorite_filter(true);
    assert_eq!(state.category(), ContentCategory::All);
    assert!(state.favorite_only());
    state.set_group(Some(7));
    assert!(!state.favorite_only());
    assert_eq!(state.category(), ContentCategory::All);
}

#[test]
fn hiding_category_tabs_preserves_group_selection_and_resets_filters() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("history.db"))?;
    let id = history.capture("selected entry")?.unwrap();
    let group = history.create_group("Selected group")?;
    history.move_to_group(id, None, Some(group.id))?;
    let items = history.list_in_group("", PAGE_SIZE, false, Some(group.id))?;
    let mut state = HistoryState::default();

    state.set_group(Some(group.id));
    assert!(state.apply(items, 1, state.generation));
    assert_eq!(state.select_index(0), Some(0));
    let group_generation = state.generation;
    for _ in 0..2 {
        assert!(!state.should_reset_category_filter(false));
        assert_eq!(state.group_id(), Some(group.id));
        assert_eq!(state.selected, Some(id));
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.generation, group_generation);
    }

    for category in [ContentCategory::Text, ContentCategory::Other] {
        state.set_category(category);
        assert!(!state.should_reset_category_filter(true));
        assert_eq!(state.category(), category);
        assert!(state.should_reset_category_filter(false));
        state.set_category(ContentCategory::All);
        assert_eq!(state.category(), ContentCategory::All);
        assert!(!state.should_reset_category_filter(false));
    }

    state.set_favorite_filter(true);
    assert!(!state.should_reset_category_filter(true));
    assert!(state.favorite_only());
    assert!(state.should_reset_category_filter(false));
    state.set_category(ContentCategory::All);
    assert!(!state.favorite_only());
    assert!(!state.should_reset_category_filter(false));
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
    state.apply(history.list("", PAGE_SIZE, false)?, 2, 0);
    state.selected = Some(a);
    history.capture("alpha")?;
    state.apply(history.list("", PAGE_SIZE, false)?, 2, 0);
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
    state.begin_search();
    assert!(!state.apply(history.list("", PAGE_SIZE, false)?, 2, 0));
    assert!(state.items.is_empty());
    assert!(state.apply(history.list("beta", PAGE_SIZE, false)?, 1, 1));
    assert_eq!(state.selected, Some(b));
    history.delete(b)?;
    state.apply(vec![], 0, 1);
    assert_eq!(state.select_relative(1), None);
    assert_eq!(state.select_index(0), None);
    assert_eq!(state.selected, None);
    Ok(())
}
