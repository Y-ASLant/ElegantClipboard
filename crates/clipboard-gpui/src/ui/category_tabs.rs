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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolbarTab {
    Category(Option<ContentCategory>),
    Todos,
}

impl ClipboardView {
    pub(super) fn toolbar_tabs(&self) -> Vec<ToolbarTab> {
        visible_toolbar_tabs(
            &self.display.value.category_tabs,
            self.todo_preference.value,
        )
    }

    pub(super) fn toolbar_tab_index(&self) -> usize {
        let active = if self.todos_open {
            ToolbarTab::Todos
        } else if self.history.favorite_only() {
            ToolbarTab::Category(None)
        } else {
            ToolbarTab::Category(Some(self.history.category()))
        };
        self.toolbar_tabs()
            .iter()
            .position(|tab| *tab == active)
            .unwrap_or(0)
    }

    pub(super) fn toolbar_tab_text(&self, tab: ToolbarTab) -> (&'static str, &'static str) {
        match tab {
            ToolbarTab::Todos => (
                tr(self.language, "待办", "To-do"),
                tr(self.language, "待办", "To-do"),
            ),
            ToolbarTab::Category(filter) => CATEGORY_TAB_SPECS
                .iter()
                .find(|(spec, ..)| *spec == filter)
                .map(|(_, chinese, english, accessible)| {
                    (
                        tr(self.language, chinese, english),
                        tr(self.language, chinese, accessible),
                    )
                })
                .unwrap_or(("全部", "All")),
        }
    }

    pub(super) fn active_category_tab_visible(&self) -> bool {
        if !self.display.value.show_category_filter {
            return true;
        }
        let tabs = &self.display.value.category_tabs;
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

fn visible_toolbar_tabs(
    categories: &CategoryTabVisibility,
    todo: TodoPreference,
) -> Vec<ToolbarTab> {
    let mut tabs = Vec::new();
    for (index, (visible, (filter, ..))) in category_tab_flags(categories)
        .into_iter()
        .zip(CATEGORY_TAB_SPECS)
        .enumerate()
    {
        if visible {
            tabs.push(ToolbarTab::Category(*filter));
        }
        if index == 1 && todo.enabled && todo.tab_position == TodoTabPosition::AfterFavorites {
            tabs.push(ToolbarTab::Todos);
        }
    }
    if todo.enabled && todo.tab_position == TodoTabPosition::Last {
        tabs.push(ToolbarTab::Todos);
    }
    tabs
}

#[cfg(test)]
mod tests {
    use super::{ToolbarTab, visible_toolbar_tabs};
    use clipboard_core::ContentCategory;
    use clipboard_core::preferences::{CategoryTabVisibility, TodoPreference, TodoTabPosition};

    #[test]
    fn todo_visibility_and_position_respect_hidden_categories() {
        let categories = CategoryTabVisibility {
            all: true,
            favorites: false,
            text: false,
            image: true,
            url: false,
            other: true,
        };
        let all = ToolbarTab::Category(Some(ContentCategory::All));
        let image = ToolbarTab::Category(Some(ContentCategory::Image));
        let other = ToolbarTab::Category(Some(ContentCategory::Other));
        assert_eq!(
            visible_toolbar_tabs(&categories, TodoPreference::default()),
            vec![all, image, other, ToolbarTab::Todos]
        );
        assert_eq!(
            visible_toolbar_tabs(
                &categories,
                TodoPreference {
                    tab_position: TodoTabPosition::AfterFavorites,
                    ..Default::default()
                }
            ),
            vec![all, ToolbarTab::Todos, image, other]
        );
        for tab_position in [TodoTabPosition::AfterFavorites, TodoTabPosition::Last] {
            assert_eq!(
                visible_toolbar_tabs(
                    &categories,
                    TodoPreference {
                        enabled: false,
                        tab_position,
                        ..Default::default()
                    }
                ),
                vec![all, image, other]
            );
        }
    }
}
