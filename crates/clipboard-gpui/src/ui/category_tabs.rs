use super::*;
use clipboard_core::ContentCategory;

pub(super) struct CategoryTabEntry {
    pub filter: Option<ContentCategory>,
    pub label: String,
    pub accessible: String,
}

impl ClipboardView {
    pub(super) fn category_tab_entries(&self) -> Vec<CategoryTabEntry> {
        let tabs = &self.display.category_tabs;
        let mut entries = Vec::new();
        if tabs.all {
            entries.push(CategoryTabEntry {
                filter: Some(ContentCategory::All),
                label: tr(self.language, "全部", "All").to_string(),
                accessible: tr(self.language, "全部", "All").to_string(),
            });
        }
        if tabs.favorites {
            entries.push(CategoryTabEntry {
                filter: None,
                label: tr(self.language, "收藏", "Favs").to_string(),
                accessible: tr(self.language, "收藏", "Favorites").to_string(),
            });
        }
        if tabs.text {
            entries.push(CategoryTabEntry {
                filter: Some(ContentCategory::Text),
                label: tr(self.language, "文本", "Text").to_string(),
                accessible: tr(self.language, "文本", "Text").to_string(),
            });
        }
        if tabs.other {
            entries.push(CategoryTabEntry {
                filter: Some(ContentCategory::Other),
                label: tr(self.language, "其他", "Other").to_string(),
                accessible: tr(self.language, "其他", "Other").to_string(),
            });
        }
        entries
    }

    pub(super) fn category_tab_index(&self) -> usize {
        let entries = self.category_tab_entries();
        if entries.is_empty() {
            return 0;
        }
        let active = if self.history.favorite_only() {
            None
        } else {
            Some(self.history.category())
        };
        entries
            .iter()
            .position(|entry| entry.filter == active)
            .unwrap_or(0)
    }

    pub(super) fn active_category_tab_visible(&self) -> bool {
        if !self.display.show_category_filter {
            return true;
        }
        let tabs = &self.display.category_tabs;
        if self.history.favorite_only() {
            return tabs.favorites;
        }
        match self.history.category() {
            ContentCategory::All => tabs.all,
            ContentCategory::Text => tabs.text,
            ContentCategory::Other => tabs.other,
        }
    }
}
