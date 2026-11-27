use chrono::{DateTime, Local, NaiveDateTime};
use clipboard_core::{
    ContentCategory, PreviewContent,
    database::ClipboardItem,
    preferences::{LanguagePreference, SourceAppDisplay, TimeFormat},
};
use indexmap::IndexSet;
use std::collections::HashMap;

mod items;
pub use items::HistoryItems;

/// Return whether to show the name and icon; icon-only falls back to the name.
pub fn source_app_parts(mode: SourceAppDisplay, has_icon: bool) -> (bool, bool) {
    match mode {
        SourceAppDisplay::Both => (true, has_icon),
        SourceAppDisplay::Name => (true, false),
        SourceAppDisplay::Icon => (!has_icon, has_icon),
    }
}

/// Labels are `app` or `app：url/doc`. Cards pass `false`; hover uses `HoverPreviewDetails::source_url`.
pub fn source_app_visible_name(stored: &str, show_source_url: bool) -> &str {
    if show_source_url {
        return stored;
    }
    stored.split_once('：').map_or(stored, |(name, _)| name)
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

/// Append newly marked rows in click order, or from the anchor toward the Shift-click.
pub(crate) fn toggle_batch_mark(
    selected: &mut IndexSet<i64>,
    items: &HistoryItems,
    anchor: Option<i64>,
    target: i64,
) -> bool {
    let Some(to) = items.known_index(target) else {
        return false;
    };
    if let Some(from) = anchor.and_then(|id| items.known_index(id)) {
        // Evicting metadata doesn't unmark IDs from a previously visited page.
        let range = items.ids[from.min(to)..=from.max(to)]
            .iter()
            .flatten()
            .copied();
        if from <= to {
            selected.extend(range);
        } else {
            selected.extend(range.rev());
        }
    } else if !selected.shift_remove(&target) {
        selected.insert(target);
    }
    true
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

/// Leave All drawable during the outbound query. Only move its storage out
/// when the first Favorites reply actually replaces the displayed rows.
enum ParkedAll {
    OnScreen,
    OffScreen(HistoryItems),
}

/// View state kept independent of GPUI so asynchronous ordering can be tested.
///
/// The full filtered order is addressable while only a window of metadata
/// remains resident. Old rows stay drawable until a new view's first reply.
pub struct HistoryState {
    pub items: HistoryItems,
    pub total: i64,
    pub generation: u64,
    pub selected: Option<i64>,
    pub loading: bool,
    filter: HistoryFilter,
    group_id: Option<i64>,
    parked_all: Option<ParkedAll>,
    // The return query still has to synchronize the worker's scope and order.
    // Until it replies, the restored rows may be parked again but not paged.
    restored_all: bool,
    // The generation whose replies most recently wrote into the slots. A new
    // view's first page resets the slots: positions in the previous view are
    // unrelated, and a merge would pair the wrong rows with their indexes.
    slots_generation: Option<u64>,
    // Last row the keyboard reached, loaded or not. Slots never move inside a
    // view, so the index stays valid while pages stream in around it.
    navigation_index: Option<usize>,
}

impl Default for HistoryState {
    fn default() -> Self {
        Self {
            items: HistoryItems::default(),
            total: 0,
            generation: 0,
            selected: None,
            loading: true,
            filter: HistoryFilter::All,
            group_id: None,
            parked_all: None,
            restored_all: false,
            slots_generation: None,
            navigation_index: None,
        }
    }
}

/// A new worker scope that still needs `Command::Query`, even if already drawable.
#[must_use = "arm a history query to synchronize the worker's scope"]
pub struct HistoryReload {
    generation: u64,
}

impl HistoryReload {
    pub(crate) fn generation(self) -> u64 {
        self.generation
    }
}

impl HistoryState {
    pub fn item_index(&self, id: i64) -> Option<usize> {
        self.items
            .loaded()
            .find_map(|(i, row)| (row.id == id).then_some(i))
    }

    pub fn can_page(&self) -> bool {
        !self.loading && self.slots_generation == Some(self.generation)
    }

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

    pub(crate) fn parked_all_items(&self) -> Option<&HistoryItems> {
        match self.parked_all.as_ref()? {
            ParkedAll::OnScreen => Some(&self.items),
            ParkedAll::OffScreen(items) => Some(items),
        }
    }

    /// A companion page read in the same worker scope as Favorites. Its order
    /// is authoritative for All; never merge it with positions from a prior read.
    pub(crate) fn remember_all_page(
        &mut self,
        items: Vec<ClipboardItem>,
        total: i64,
        generation: u64,
    ) {
        if generation != self.generation || !self.favorite_only() {
            return;
        }
        let mut all = match self.parked_all.take() {
            Some(ParkedAll::OffScreen(items)) => items,
            _ => HistoryItems::default(),
        };
        all.watch(0..1);
        all.merge(items, usize::try_from(total).unwrap_or(0), 0, true, true);
        self.parked_all = Some(ParkedAll::OffScreen(all));
    }

    /// An effective search change (including the locate override) invalidates
    /// All even when its unchanged tab and group would otherwise match.
    pub(crate) fn invalidate_all_view(&mut self) {
        self.parked_all = None;
        self.restored_all = false;
        if self.filter == HistoryFilter::All {
            self.slots_generation = None;
        }
    }

    /// `None` leaves generation and any in-flight snapshot untouched.
    fn set_filter(&mut self, filter: HistoryFilter) -> Option<HistoryReload> {
        if self.filter == filter {
            return None;
        }
        let parked = match (self.filter, filter) {
            (HistoryFilter::All, HistoryFilter::Favorites)
                if (self.can_page() || self.restored_all)
                    && (self.items.is_empty() || self.items[0].is_some()) =>
            {
                Some(ParkedAll::OnScreen)
            }
            (HistoryFilter::Favorites, HistoryFilter::All)
                if self
                    .parked_all_items()
                    .is_some_and(|items| items.is_empty() || items[0].is_some()) =>
            {
                self.parked_all.take()
            }
            _ => None,
        };
        self.filter = filter;
        let reload = self.begin_search();
        if let Some(parked) = parked {
            if filter == HistoryFilter::Favorites {
                self.parked_all = Some(parked);
            } else {
                if let ParkedAll::OffScreen(items) = parked {
                    self.items = items;
                }
                self.items.watch(0..1);
                self.total = self.items.len() as i64;
                self.selected = self.items.id_at(0);
                self.loading = false;
                self.restored_all = true;
            }
        }
        Some(reload)
    }

    #[must_use = "arm the history query to synchronize the worker's scope"]
    pub fn set_favorite_filter(&mut self, favorite_only: bool) -> Option<HistoryReload> {
        self.set_filter(if favorite_only {
            HistoryFilter::Favorites
        } else {
            HistoryFilter::All
        })
    }

    #[must_use = "arm the history query to synchronize the worker's scope"]
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

    /// Rows stay in place while the replacement is in flight: the previous
    /// view's data remains visible until the new snapshot swaps it atomically,
    /// so switching scopes never blanks the list.
    pub fn begin_search(&mut self) -> HistoryReload {
        self.parked_all = None;
        self.restored_all = false;
        self.generation += 1;
        self.loading = true;
        self.selected = None;
        self.navigation_index = None;
        HistoryReload {
            generation: self.generation,
        }
    }

    /// Merge one paged snapshot into the sparse slots. `structural` snapshots
    /// follow mutations and captures: the worker re-reads the authoritative
    /// order, so every slot outside the page may have shifted and is dropped.
    /// Query answers only page in data and merge into the surviving slots.
    pub fn apply(
        &mut self,
        items: Vec<ClipboardItem>,
        total: i64,
        offset: usize,
        structural: bool,
        generation: u64,
    ) -> bool {
        if generation != self.generation {
            return false;
        }
        if structural {
            // A filtered mutation reply says nothing about All's new order.
            self.parked_all = None;
        } else if matches!(self.parked_all, Some(ParkedAll::OnScreen)) {
            self.parked_all = Some(ParkedAll::OffScreen(std::mem::take(&mut self.items)));
        }
        self.restored_all = false;
        let total_len = usize::try_from(total).unwrap_or(0);
        let new_view = self.slots_generation != Some(generation);
        if new_view {
            self.items.watch(offset..offset.saturating_add(1));
        } else if !structural && !self.items.wants_page(offset) {
            return false;
        }
        self.slots_generation = Some(generation);
        self.items.merge(
            items,
            total_len,
            offset,
            structural || new_view,
            self.filter == HistoryFilter::All,
        );
        if self
            .selected
            .is_none_or(|id| self.items.known_index(id).is_none())
        {
            self.selected = match self.navigation_index {
                Some(i) if !structural && !new_view => self.items.id_at(i),
                _ => self
                    .items
                    .id_at(offset)
                    .or_else(|| self.items.known_ids().next()),
            };
        }
        self.total = total;
        self.loading = false;
        true
    }
    /// Reconcile a committed deletion without completing an in-flight query.
    pub fn apply_deleted(&mut self, id: i64) -> bool {
        // Return whether the *displayed* rows changed. A delete can affect
        // only parked All; if its position is unknown, don't restore its slots.
        let removed = self.items.remove(id);
        let parked_valid = match self.parked_all.as_mut() {
            None => true,
            Some(ParkedAll::OnScreen) => removed || !self.items.ids.contains(&None),
            Some(ParkedAll::OffScreen(items)) => items.remove(id) || !items.ids.contains(&None),
        };
        if !parked_valid {
            self.parked_all = None;
        }
        if !removed {
            return false;
        }
        self.total = self.total.saturating_sub(1).max(0);
        if self.selected == Some(id) {
            self.selected = self.items.known_ids().next();
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
        let len = self.items.len();
        if len == 0 {
            self.selected = None;
            return None;
        }
        let current = self
            .selected
            .and_then(|id| self.items.known_index(id))
            .or(self.navigation_index)
            .unwrap_or(0);
        let index = current.saturating_add_signed(direction).min(len - 1);
        self.navigation_index = Some(index);
        self.selected = self.items.id_at(index);
        Some(index)
    }

    pub fn select_index(&mut self, index: usize) -> Option<usize> {
        let index = index.min(self.items.len().checked_sub(1)?);
        self.navigation_index = Some(index);
        self.selected = self.items.id_at(index);
        Some(index)
    }
}

#[cfg(test)]
fn settle(reload: impl Sized) {
    let _ = std::mem::size_of_val(&reload);
}

#[cfg(test)]
mod deletion_tests;
#[cfg(test)]
mod paging_tests;
#[cfg(test)]
mod tests;
