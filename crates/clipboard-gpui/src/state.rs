use clipboard_core::{PAGE_SIZE, database::ClipboardItem};

#[derive(Default)]
pub struct PreviewState {
    pub id: Option<i64>,
    pub generation: u64,
    pub result: Option<Result<String, String>>,
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

    pub fn apply(&mut self, id: i64, generation: u64, result: Result<String, String>) -> bool {
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
        }
    }
}

impl HistoryState {
    pub fn set_favorite_filter(&mut self, favorite_only: bool) {
        self.favorite_only = favorite_only;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::History;

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
    }

    #[test]
    fn preview_rejects_closed_switched_and_reopened_requests() {
        let mut preview = PreviewState::default();
        let first = preview.open(1);
        preview.close();
        assert!(!preview.apply(1, first, Ok("late".into())));
        let second = preview.open(1);
        assert!(!preview.apply(1, first, Ok("old".into())));
        assert!(preview.apply(1, second, Ok("full text".into())));
        let third = preview.open(2);
        assert!(preview.result.is_none());
        assert!(!preview.apply(1, second, Ok("old selection".into())));
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
        state.begin_search();
        assert!(!state.apply(history.list("", PAGE_SIZE, false)?, 2, 0));
        assert!(state.items.is_empty());
        assert!(state.apply(history.list("beta", PAGE_SIZE, false)?, 1, 1));
        assert_eq!(state.selected, Some(b));
        history.delete(b)?;
        state.apply(vec![], 0, 1);
        assert_eq!(state.select_relative(1), None);
        assert_eq!(state.selected, None);
        Ok(())
    }
}
