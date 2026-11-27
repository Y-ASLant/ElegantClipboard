use clipboard_core::{HISTORY_PAGE_ROWS, database::ClipboardItem};
use std::{collections::VecDeque, ops::Range};

// Keep a little room for direction changes. Only the visible/prefetch pages are fetched.
const WINDOW_ROWS: usize = 4 * HISTORY_PAGE_ROWS;

/// Geometry and remembered IDs use absolute slots; only a bounded, contiguous
/// window can own metadata. A jump never allocates the intervening rows.
#[derive(Default)]
pub struct HistoryItems {
    pub(super) ids: Vec<Option<i64>>,
    rows: VecDeque<Option<ClipboardItem>>,
    start: usize,
    window_start: usize,
}

impl HistoryItems {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Option<ClipboardItem>> {
        (index < self.len()).then(|| {
            index
                .checked_sub(self.start)
                .and_then(|slot| self.rows.get(slot))
                .unwrap_or(&None)
        })
    }

    #[cfg(test)]
    pub(super) fn metadata_capacity(&self) -> usize {
        self.rows.capacity()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Option<ClipboardItem>> {
        std::iter::repeat_n(&None, self.start)
            .chain(self.rows.iter())
            .chain(std::iter::repeat_n(
                &None,
                self.len() - self.start - self.rows.len(),
            ))
    }

    pub fn loaded(&self) -> impl DoubleEndedIterator<Item = (usize, &ClipboardItem)> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(move |(i, row)| Some((self.start + i, row.as_ref()?)))
    }

    pub fn loaded_range(&self, range: Range<usize>) -> impl Iterator<Item = &ClipboardItem> {
        let start = range.start.saturating_sub(self.start).min(self.rows.len());
        let end = range.end.saturating_sub(self.start).min(self.rows.len());
        self.rows.range(start..end).flatten()
    }

    pub fn known_ids(&self) -> impl DoubleEndedIterator<Item = i64> + '_ {
        self.ids.iter().flatten().copied()
    }

    pub fn known_rows(&self) -> impl Iterator<Item = (usize, i64)> + '_ {
        self.ids
            .iter()
            .enumerate()
            .filter_map(|(i, id)| Some((i, (*id)?)))
    }

    pub(super) fn known_index(&self, id: i64) -> Option<usize> {
        self.ids.iter().position(|slot| *slot == Some(id))
    }

    pub fn id_at(&self, index: usize) -> Option<i64> {
        self.ids.get(index).copied().flatten()
    }

    pub fn wants_page(&self, offset: usize) -> bool {
        (self.window_start..self.window_start.saturating_add(WINDOW_ROWS)).contains(&offset)
    }

    /// Move the acceptance window before sending requests, not when replies land.
    /// Old rows remain drawable until the first new page swaps them out.
    pub fn watch(&mut self, range: Range<usize>) {
        if !self.wants_page(range.start) || !self.wants_page(range.end.saturating_sub(1)) {
            let first = range.start / HISTORY_PAGE_ROWS;
            let end_page = range.end.div_ceil(HISTORY_PAGE_ROWS);
            self.window_start = first
                .saturating_sub(1)
                .max(end_page.saturating_sub(WINDOW_ROWS / HISTORY_PAGE_ROWS))
                .min(first)
                * HISTORY_PAGE_ROWS;
        }
    }

    pub(super) fn merge(
        &mut self,
        items: Vec<ClipboardItem>,
        total: usize,
        offset: usize,
        reset: bool,
    ) {
        if reset {
            self.ids.clear();
            self.rows.clear();
        }
        self.ids.resize(total, None);
        let lower = self.window_start.min(total);
        let upper = self.window_start.saturating_add(WINDOW_ROWS).min(total);
        self.rows.truncate(upper.saturating_sub(self.start));
        let drop = lower.saturating_sub(self.start).min(self.rows.len());
        self.rows.drain(..drop);
        self.start += drop;
        if self.rows.is_empty() {
            self.start = offset.clamp(lower, upper);
        }

        let end = offset.saturating_add(items.len()).min(total);
        let from = lower.max(offset);
        let to = upper.min(end);
        if from < to {
            for _ in from..self.start {
                self.rows.push_front(None);
            }
            self.start = self.start.min(from);
            if to > self.start + self.rows.len() {
                self.rows.resize_with(to - self.start, || None);
            }
        }
        for (i, item) in (offset..end).zip(items) {
            self.ids[i] = Some(item.id);
            if (from..to).contains(&i) {
                self.rows[i - self.start] = Some(item);
            }
        }
    }

    pub(super) fn remove(&mut self, id: i64) -> bool {
        let Some(index) = self.known_index(id) else {
            return false;
        };
        self.ids.remove(index);
        if index < self.start {
            self.start -= 1;
        } else if index - self.start < self.rows.len() {
            self.rows.remove(index - self.start);
        }
        true
    }
}

impl std::ops::Index<usize> for HistoryItems {
    type Output = Option<ClipboardItem>;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
            .expect("history row outside the current view")
    }
}
