use chrono::{DateTime, Local, NaiveDateTime};
use clipboard_core::{
    ContentCategory, PreviewContent,
    database::ClipboardItem,
    preferences::{LanguagePreference, SourceAppDisplay, TimeFormat},
};
use std::collections::HashMap;

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

/// The mutually exclusive category tabs; the selected group is independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryFilter {
    All,
    Favorites,
    Category(ContentCategory),
}

/// View state kept independent of GPUI so asynchronous ordering can be tested.
pub struct HistoryState {
    pub items: Vec<ClipboardItem>,
    pub total: i64,
    pub generation: u64,
    pub selected: Option<i64>,
    pub loading: bool,
    filter: HistoryFilter,
    group_id: Option<i64>,
}

impl Default for HistoryState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            total: 0,
            generation: 0,
            selected: None,
            loading: true,
            filter: HistoryFilter::All,
            group_id: None,
        }
    }
}

/// A history generation that is loading and still needs `Command::Query`.
#[must_use = "arm a history query or the list stays loading"]
pub struct HistoryReload {
    generation: u64,
}

impl HistoryReload {
    pub(crate) fn generation(self) -> u64 {
        self.generation
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
        self.group_id
    }

    pub fn should_reset_category_filter(&self, show_category_filter: bool) -> bool {
        !show_category_filter && (self.favorite_only() || self.category() != ContentCategory::All)
    }

    /// `None` leaves generation and any in-flight snapshot untouched.
    fn set_filter(&mut self, filter: HistoryFilter) -> Option<HistoryReload> {
        if self.filter == filter {
            return None;
        }
        self.filter = filter;
        Some(self.begin_search())
    }

    #[must_use = "arm a history query or the list stays loading"]
    pub fn set_favorite_filter(&mut self, favorite_only: bool) -> Option<HistoryReload> {
        self.set_filter(if favorite_only {
            HistoryFilter::Favorites
        } else {
            HistoryFilter::All
        })
    }

    #[must_use = "arm a history query or the list stays loading"]
    pub fn set_category(&mut self, category: ContentCategory) -> Option<HistoryReload> {
        self.set_filter(match category {
            ContentCategory::All => HistoryFilter::All,
            category => HistoryFilter::Category(category),
        })
    }

    #[must_use = "arm a history query or the list stays loading"]
    pub fn set_group(&mut self, group_id: Option<i64>) -> Option<HistoryReload> {
        if self.group_id == group_id {
            return None;
        }
        self.group_id = group_id;
        Some(self.begin_search())
    }

    #[must_use = "arm a history query or the list stays loading"]
    pub fn reset_filters(&mut self) -> HistoryReload {
        self.group_id = None;
        self.filter = HistoryFilter::All;
        self.begin_search()
    }

    #[must_use = "arm a history query or the list stays loading"]
    pub fn begin_search(&mut self) -> HistoryReload {
        self.generation += 1;
        self.loading = true;
        self.items.clear();
        self.selected = None;
        self.total = 0;
        HistoryReload {
            generation: self.generation,
        }
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

    /// Reconcile a committed deletion without completing an in-flight query.
    pub fn apply_deleted(&mut self, id: i64) -> bool {
        if !remove_deleted_item(&mut self.items, &mut self.total, id) {
            return false;
        }
        if self.selected == Some(id) {
            self.selected = self.items.first().map(|item| item.id);
        }
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

/// Remove only a loaded ID: an absent ID may belong to another filter or an
/// already refreshed snapshot, whose total must not be decremented again.
pub fn remove_deleted_item(items: &mut Vec<ClipboardItem>, total: &mut i64, id: i64) -> bool {
    let Some(index) = items.iter().position(|item| item.id == id) else {
        return false;
    };
    items.remove(index);
    *total = total.saturating_sub(1).max(0);
    true
}

#[cfg(test)]
fn settle(reload: impl Sized) {
    let _ = std::mem::size_of_val(&reload);
}

#[cfg(test)]
mod deletion_tests;
#[cfg(test)]
mod tests;
