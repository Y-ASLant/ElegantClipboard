use chrono::{DateTime, Local, NaiveDateTime};
use clipboard_core::{
    ContentCategory, PAGE_SIZE, PreviewContent,
    database::ClipboardItem,
    preferences::{LanguagePreference, SourceAppDisplay, TimeFormat},
};
use std::{collections::HashMap, ops::Range};

/// Return whether to show the name and icon; icon-only falls back to the name.
pub fn source_app_parts(mode: SourceAppDisplay, has_icon: bool) -> (bool, bool) {
    match mode {
        SourceAppDisplay::Both => (true, has_icon),
        SourceAppDisplay::Name => (true, false),
        SourceAppDisplay::Icon => (!has_icon, has_icon),
    }
}

pub fn format_card_time(
    created_at: &str,
    format: TimeFormat,
    language: LanguagePreference,
    now: NaiveDateTime,
) -> String {
    let Some(date) = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%d %H:%M:%S")
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(created_at)
                .ok()
                .map(|date| date.with_timezone(&Local).naive_local())
        })
    else {
        return created_at.to_owned();
    };
    if format == TimeFormat::Relative {
        let seconds = now.signed_duration_since(date).num_seconds().max(0);
        let (count, chinese, english) = match seconds {
            0..60 => {
                return if language == LanguagePreference::Chinese {
                    "刚刚".into()
                } else {
                    "Just now".into()
                };
            }
            60..3600 => (seconds / 60, " 分钟前", " min ago"),
            3600..86400 => (seconds / 3600, " 小时前", " hr ago"),
            86400..2592000 => (seconds / 86400, " 天前", " days ago"),
            2592000..31104000 => (seconds / 2592000, " 个月前", " mo ago"),
            _ => (seconds / 31104000, " 年前", " yr ago"),
        };
        return if language == LanguagePreference::Chinese {
            format!("{count}{chinese}")
        } else {
            format!("{count}{english}")
        };
    }

    let time = date.format("%H:%M");
    if date.date() == now.date() {
        format!(
            "{} {time}",
            if language == LanguagePreference::Chinese {
                "今天"
            } else {
                "Today"
            }
        )
    } else if now.date().pred_opt() == Some(date.date()) {
        format!(
            "{} {time}",
            if language == LanguagePreference::Chinese {
                "昨天"
            } else {
                "Yesterday"
            }
        )
    } else {
        date.format("%m-%d %H:%M").to_string()
    }
}

/// Non-overlapping UTF-8 ranges for a literal term. A limit lets callers find
/// the first match in a long body without collecting every occurrence.
fn matching_ranges(text: &str, search: &str, limit: usize) -> Vec<Range<usize>> {
    if search.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let needle: Vec<char> = search.chars().collect();
    let mut ranges = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, first)) = chars.next() {
        let mut candidate = chars.clone();
        let mut end = start + first.len_utf8();
        let matched = first.to_lowercase().eq(needle[0].to_lowercase())
            && needle.iter().skip(1).all(|expected| {
                if let Some((byte, actual)) = candidate.next()
                    && actual.to_lowercase().eq(expected.to_lowercase())
                {
                    end = byte + actual.len_utf8();
                    true
                } else {
                    false
                }
            });
        if matched {
            ranges.push(start..end);
            if ranges.len() == limit {
                break;
            }
            chars = candidate;
        }
    }
    ranges
}

/// Search matches in a card preview, with byte ranges safe for StyledText.
pub fn search_highlight_ranges(text: &str, search: &str) -> Vec<Range<usize>> {
    matching_ranges(text, search, usize::MAX)
}

/// Show the first matching part of a full text record when its saved preview
/// does not contain the term. Keep the match near the start of a two-line card.
pub fn search_excerpt(full: &str, preview: &str, search: &str) -> Option<String> {
    if !matching_ranges(preview, search, 1).is_empty() {
        return None;
    }
    let matched = matching_ranges(full, search, 1).pop()?;
    let start = full[..matched.start]
        .char_indices()
        .rev()
        .nth(19)
        .map_or(0, |(byte, _)| byte);
    let end = full[matched.end..]
        .char_indices()
        .nth(80)
        .map_or(full.len(), |(byte, _)| matched.end + byte);
    let mut excerpt = String::new();
    if start > 0 {
        excerpt.push('…');
    }
    excerpt.push_str(&full[start..end]);
    if end < full.len() {
        excerpt.push('…');
    }
    Some(excerpt)
}

/// IDs between two loaded rows, inclusive, in their current display order.
/// An unloaded or filtered-out anchor does not define a range.
pub fn selection_range_ids(ids: &[i64], anchor: i64, target: i64) -> Option<&[i64]> {
    let from = ids.iter().position(|id| *id == anchor)?;
    let to = ids.iter().position(|id| *id == target)?;
    Some(&ids[from.min(to)..=from.max(to)])
}

/// Pixel offsets from the old order to the committed order, using each row's actual height.
pub fn reorder_pixel_offsets(before: &[i64], after: &[(i64, f32)]) -> HashMap<i64, f32> {
    if before.len() != after.len() {
        return HashMap::new();
    }
    let mut positions = HashMap::with_capacity(after.len());
    let mut top = 0.;
    for &(id, height) in after {
        if positions.insert(id, (top, height)).is_some() {
            return HashMap::new();
        }
        top += height;
    }
    let mut old_top = 0.;
    let mut offsets = HashMap::new();
    for id in before {
        let Some(&(new_top, height)) = positions.get(id) else {
            return HashMap::new();
        };
        if old_top != new_top {
            offsets.insert(*id, old_top - new_top);
        }
        old_top += height;
    }
    offsets
}

/// Advance a virtual list by one row without leaving the loaded item range.
pub fn next_drag_scroll_index(current: usize, item_count: usize, direction: i8) -> usize {
    if item_count == 0 || direction == 0 {
        return current;
    }
    if direction < 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(item_count - 1)
    }
}

/// Resolve the row under the active edge after programmatic virtual-list scrolling.
pub fn drag_edge_target_index(
    scroll_top: usize,
    item_count: usize,
    visible_span: usize,
    direction: i8,
) -> Option<usize> {
    if item_count == 0 || direction == 0 {
        return None;
    }
    Some(if direction < 0 {
        scroll_top.min(item_count - 1)
    } else {
        scroll_top.saturating_add(visible_span).min(item_count - 1)
    })
}

#[derive(Default)]
pub struct PreviewState {
    pub id: Option<i64>,
    pub generation: u64,
    pub result: Option<Result<PreviewContent, String>>,
}

impl PreviewState {
    pub fn open(&mut self, id: i64) -> u64 {
        self.generation += 1;
        self.id = Some(id);
        self.result = None;
        self.generation
    }

    pub fn close(&mut self) {
        self.id = None;
        self.result = None;
    }

    pub fn apply(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
    ) -> bool {
        if self.id != Some(id) || self.generation != generation {
            return false;
        }
        self.result = Some(result);
        true
    }
}

/// 历史列表的互斥筛选维度：任意时刻只作用于一个维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFilter {
    All,
    Favorites,
    Category(ContentCategory),
    Group(i64),
}

/// View state kept independent of GPUI so asynchronous ordering can be tested.
pub struct HistoryState {
    pub items: Vec<ClipboardItem>,
    pub total: i64,
    pub generation: u64,
    pub limit: i64,
    pub selected: Option<i64>,
    pub loading: bool,
    filter: HistoryFilter,
}

impl Default for HistoryState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            generation: 0,
            limit: PAGE_SIZE,
            selected: None,
            loading: true,
            filter: HistoryFilter::All,
        }
    }
}

impl HistoryState {
    pub fn favorite_only(&self) -> bool {
        matches!(self.filter, HistoryFilter::Favorites)
    }

    pub fn category(&self) -> ContentCategory {
        match self.filter {
            HistoryFilter::Category(category) => category,
            _ => ContentCategory::All,
        }
    }

    pub fn group_id(&self) -> Option<i64> {
        match self.filter {
            HistoryFilter::Group(id) => Some(id),
            _ => None,
        }
    }

    pub fn should_reset_category_filter(&self, show_category_filter: bool) -> bool {
        !show_category_filter
            && self.group_id().is_none()
            && (self.favorite_only() || self.category() != ContentCategory::All)
    }

    pub fn set_filter(&mut self, filter: HistoryFilter) {
        self.filter = filter;
        self.begin_search();
    }

    pub fn set_favorite_filter(&mut self, favorite_only: bool) {
        self.set_filter(if favorite_only {
            HistoryFilter::Favorites
        } else {
            HistoryFilter::All
        });
    }

    pub fn set_category(&mut self, category: ContentCategory) {
        self.set_filter(HistoryFilter::Category(category));
    }

    pub fn set_group(&mut self, group_id: Option<i64>) {
        self.set_filter(group_id.map_or(HistoryFilter::All, HistoryFilter::Group));
    }

    pub fn begin_search(&mut self) {
        self.generation += 1;
        self.limit = PAGE_SIZE;
        self.loading = true;
        self.items.clear();
        self.selected = None;
        self.total = 0;
    }

    pub fn apply(&mut self, items: Vec<ClipboardItem>, total: i64, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        if !items.iter().any(|item| Some(item.id) == self.selected) {
            self.selected = items.first().map(|item| item.id);
        }
        self.items = items;
        self.total = total;
        self.loading = false;
        true
    }

    pub fn fail_query(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        true
    }

    pub fn select_relative(&mut self, direction: isize) -> Option<usize> {
        if self.items.is_empty() {
            self.selected = None;
            return None;
        }
        let current = self
            .items
            .iter()
            .position(|item| Some(item.id) == self.selected)
            .unwrap_or(0);
        let next = current
            .saturating_add_signed(direction)
            .min(self.items.len() - 1);
        self.selected = Some(self.items[next].id);
        Some(next)
    }

    pub fn select_index(&mut self, index: usize) -> Option<usize> {
        let index = index.min(self.items.len().checked_sub(1)?);
        self.selected = Some(self.items[index].id);
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let now =
            NaiveDateTime::parse_from_str("2026-09-25 00:10:00", "%Y-%m-%d %H:%M:%S").unwrap();
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
    use clipboard_core::History;

    #[test]
    fn search_highlights_keep_utf8_boundaries_and_match_literal_case() {
        let text = "中aBc🙂中文 ABC";
        let ranges = search_highlight_ranges(text, "AbC");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["aBc", "ABC"]
        );
        let ranges = search_highlight_ranges(text, "中");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["中", "中"]
        );
        let ranges = search_highlight_ranges("a.c a-c", "a.c");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 0..3);
        assert!(search_highlight_ranges(text, "  ").is_empty());
        assert!(search_highlight_ranges(text, "missing").is_empty());
    }

    #[test]
    fn search_excerpt_exposes_a_match_beyond_the_saved_preview() {
        let full = format!("{}目标{}", "前".repeat(80), "后".repeat(120));
        let excerpt = search_excerpt(&full, "前前前", "目标").unwrap();
        assert!(excerpt.starts_with('…'));
        assert!(excerpt.ends_with('…'));
        assert!(excerpt.contains("目标"));
        assert!(!search_highlight_ranges(&excerpt, "目标").is_empty());
        assert!(search_excerpt(&full, "已显示目标", "目标").is_none());
        assert!(search_excerpt(&full, "前前前", "不存在").is_none());
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
        let offsets =
            reorder_pixel_offsets(&[4, 3, 2, 1], &[(3, 124.), (2, 88.), (4, 96.), (1, 148.)]);
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
}
