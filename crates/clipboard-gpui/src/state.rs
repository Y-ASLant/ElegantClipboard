use clipboard_core::{PAGE_SIZE, PreviewContent, database::ClipboardItem};
use std::collections::HashMap;

/// Previous row positions for items displaced by a confirmed reorder.
pub fn reorder_offsets(before: &[i64], after: &[i64]) -> HashMap<i64, isize> {
    if before.len() != after.len() {
        return HashMap::new();
    }
    let positions: HashMap<_, _> = before
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    if !after.iter().all(|id| positions.contains_key(id)) {
        return HashMap::new();
    }
    after
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let old = *positions.get(id)?;
            (old != index).then_some((*id, old as isize - index as isize))
        })
        .collect()
}

/// Row deltas for the live preview shown while an item is dragged.
/// Values are expressed in rows relative to the current list position.
pub fn drag_reorder_offsets(
    ids: &[i64],
    source: i64,
    target: i64,
    after: bool,
) -> HashMap<i64, isize> {
    let Some(source_index) = ids.iter().position(|id| *id == source) else {
        return HashMap::new();
    };
    if source == target || !ids.contains(&target) {
        return HashMap::new();
    }

    let mut preview = ids.to_vec();
    preview.remove(source_index);
    let Some(target_index) = preview.iter().position(|id| *id == target) else {
        return HashMap::new();
    };
    let insert_index = target_index + usize::from(after);
    preview.insert(insert_index, source);

    let preview_positions: HashMap<_, _> = preview
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    ids.iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let preview_index = *preview_positions.get(id)?;
            (preview_index != index).then_some((*id, preview_index as isize - index as isize))
        })
        .collect()
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

/// View state kept independent of GPUI so asynchronous ordering can be tested.
pub struct HistoryState {
    pub items: Vec<ClipboardItem>,
    pub total: i64,
    pub generation: u64,
    pub limit: i64,
    pub selected: Option<i64>,
    pub loading: bool,
    pub favorite_only: bool,
    pub group_id: Option<i64>,
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
            favorite_only: false,
            group_id: None,
        }
    }
}

impl HistoryState {
    pub fn set_favorite_filter(&mut self, favorite_only: bool) {
        self.favorite_only = favorite_only;
        self.begin_search();
    }

    pub fn set_group(&mut self, group_id: Option<i64>) {
        self.group_id = group_id;
        self.begin_search();
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
    use clipboard_core::History;

    #[test]
    fn reorder_offsets_include_every_shifted_row() {
        let offsets = reorder_offsets(&[4, 3, 2, 1], &[3, 2, 4, 1]);
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets.get(&4), Some(&-2));
        assert_eq!(offsets.get(&3), Some(&1));
        assert_eq!(offsets.get(&2), Some(&1));
        assert!(!offsets.contains_key(&1));
        assert!(reorder_offsets(&[1, 2], &[3, 2]).is_empty());
        assert!(reorder_offsets(&[1, 2], &[3, 1, 2]).is_empty());
    }

    #[test]
    fn drag_reorder_offsets_shift_rows_into_the_preview_gap() {
        assert_eq!(
            drag_reorder_offsets(&[4, 3, 2, 1], 4, 2, true),
            HashMap::from([(4, 2), (3, -1), (2, -1)])
        );
        assert_eq!(
            drag_reorder_offsets(&[4, 3, 2, 1], 1, 3, false),
            HashMap::from([(3, 1), (2, 1), (1, -2)])
        );
        assert!(drag_reorder_offsets(&[1, 2], 1, 1, false).is_empty());
        assert!(drag_reorder_offsets(&[1, 2], 3, 1, false).is_empty());
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
        assert!(state.favorite_only);
        assert_eq!(state.limit, PAGE_SIZE);
        assert_eq!(state.selected, None);
        assert!(!state.apply(vec![], 100, old));
        state.begin_search();
        assert!(state.favorite_only);
        assert!(state.apply(vec![], 0, state.generation));
        state.set_favorite_filter(false);
        assert!(!state.favorite_only);
        assert!(state.loading);
        let old = state.generation;
        state.set_group(Some(7));
        assert_eq!(state.group_id, Some(7));
        assert!(!state.apply(vec![], 1, old));
        assert!(!state.fail_query(old));
        assert!(state.loading);
        assert!(state.fail_query(state.generation));
        assert!(!state.loading);
        state.set_group(None);
        assert_eq!(state.group_id, None);
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
