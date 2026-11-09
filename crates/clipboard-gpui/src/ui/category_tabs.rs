use super::*;
use clipboard_core::ContentCategory;
use clipboard_core::preferences::CategoryTabVisibility;

/// 分类标签的静态描述：(筛选值, 中文, 英文显示名, 英文无障碍名)。
/// 「收藏」用 `None` 与 `favorites` 开关配对，且无障碍名更长（Favorites vs Favs）。
/// 主窗口工具栏的可见标签与设置页可见性复选框共用同一份文案。
pub(super) const CATEGORY_TAB_SPECS: &[(Option<ContentCategory>, &str, &str, &str)] = &[
    (Some(ContentCategory::All), "全部", "All", "All"),
    (None, "收藏", "Favs", "Favorites"),
    (Some(ContentCategory::Text), "文本", "Text", "Text"),
    (Some(ContentCategory::Image), "图片", "Image", "Image"),
    (Some(ContentCategory::Url), "链接", "Link", "Link"),
    (Some(ContentCategory::Other), "其他", "Other", "Other"),
];

/// `CATEGORY_TAB_SPECS` 与 `CategoryTabVisibility` 字段的固定配对。
pub(super) fn category_tab_flags(tabs: &CategoryTabVisibility) -> [bool; 6] {
    [
        tabs.all,
        tabs.favorites,
        tabs.text,
        tabs.image,
        tabs.url,
        tabs.other,
    ]
}

pub(super) fn set_category_tab_visible(
    tabs: &mut CategoryTabVisibility,
    index: usize,
    visible: bool,
) {
    match index {
        0 => tabs.all = visible,
        1 => tabs.favorites = visible,
        2 => tabs.text = visible,
        3 => tabs.image = visible,
        4 => tabs.url = visible,
        5 => tabs.other = visible,
        _ => {}
    }
}

pub(super) struct CategoryTabEntry {
    pub filter: Option<ContentCategory>,
    pub label: &'static str,
    pub accessible: &'static str,
}

impl ClipboardView {
    /// 可见标签的筛选值序列，顺序与 `category_tab_entries` 一致，但不分配字符串。
    /// 仅供索引进位/命中使用；需要展示文本时仍走 `category_tab_entries`。
    pub(super) fn visible_tab_filters(&self) -> impl Iterator<Item = Option<ContentCategory>> + '_ {
        category_tab_flags(&self.display.category_tabs)
            .into_iter()
            .zip(CATEGORY_TAB_SPECS)
            .filter_map(|(visible, (filter, ..))| visible.then_some(*filter))
    }

    pub(super) fn category_tab_entries(&self) -> Vec<CategoryTabEntry> {
        category_tab_flags(&self.display.category_tabs)
            .into_iter()
            .zip(CATEGORY_TAB_SPECS)
            .filter(|(visible, ..)| *visible)
            .map(
                |(_, (filter, chinese, english, accessible_en))| CategoryTabEntry {
                    filter: *filter,
                    label: tr(self.language, chinese, english),
                    accessible: tr(self.language, chinese, accessible_en),
                },
            )
            .collect()
    }

    pub(super) fn category_tab_index(&self) -> usize {
        let active = if self.history.favorite_only() {
            None
        } else {
            Some(self.history.category())
        };
        self.visible_tab_filters()
            .position(|filter| filter == active)
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
            ContentCategory::Image => tabs.image,
            ContentCategory::Url => tabs.url,
            ContentCategory::Other => tabs.other,
        }
    }
}
