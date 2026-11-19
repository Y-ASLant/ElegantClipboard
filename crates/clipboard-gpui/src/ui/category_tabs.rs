use super::*;
use clipboard_core::ContentCategory;
use clipboard_core::preferences::CategoryTabVisibility;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

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

    pub(super) fn render_category_filter(
        &self,
        available: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs = self.toolbar_tabs();
        let active = self.toolbar_tab_index();
        let labels = tabs
            .iter()
            .map(|&tab| self.toolbar_tab_text(tab))
            .collect::<Vec<_>>();
        // Kit's medium segmented rail has two 4px insets, 2px gaps and
        // 12px on either side of each label.
        let slot = (available - px(8. + 2. * (labels.len() - 1) as f32)) / labels.len() as f32;
        let font_size = visual::FILTER_LABEL_SIZE.to_pixels(window.rem_size());
        let fits = labels.iter().all(|&(label, _)| {
            visual::toolbar_text_width(label.into(), font_size, window, cx) + px(24.) <= slot
        });
        let filter = if fits {
            TabBar::new("history-category-tabs")
                .w_full()
                .segmented()
                .h(px(visual::CONTROL_HEIGHT))
                .max_width(slot)
                .last_empty_space(div().w_0())
                .selected_index(active)
                .on_click(cx.listener(|this, index: &usize, window, cx| {
                    this.select_toolbar_tab(*index, window, cx);
                }))
                .children(labels.into_iter().map(|(label, accessible)| {
                    Tab::new()
                        .min_w_0()
                        .w(slot)
                        .label(label)
                        .aria_label(accessible)
                        .flex_1()
                }))
                .into_any_element()
        } else {
            let (label, accessible) = labels[active];
            let owner = cx.weak_entity();
            let category = tr(self.language, "分类", "Category");
            Button::new("history-category-menu")
                .outline()
                .w_full()
                .h(px(visual::CONTROL_HEIGHT))
                .dropdown_caret(true)
                .accessibility_label(format!("{category}: {accessible}"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(visual::FILTER_LABEL_SIZE)
                        .child(format!("{category}: {label}")),
                )
                .dropdown_menu(move |menu, _, _| {
                    let menu = menu.min_w(available).max_w(available);
                    labels
                        .iter()
                        .enumerate()
                        .fold(menu, |menu, (index, &(_, name))| {
                            let owner = owner.clone();
                            menu.item(PopupMenuItem::new(name).checked(index == active).on_click(
                                move |_, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        this.select_toolbar_tab(index, window, cx);
                                    });
                                },
                            ))
                        })
                })
                .into_any_element()
        };
        div().flex_1().min_w_0().child(filter).into_any_element()
    }

    pub(super) fn toolbar_tab_text(&self, tab: ToolbarTab) -> (&'static str, &'static str) {
        match tab {
            ToolbarTab::Todos => (
                tr(self.language, "备忘录", "Memo"),
                tr(self.language, "备忘录", "Memo"),
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
    for (visible, (filter, ..)) in category_tab_flags(categories)
        .into_iter()
        .zip(CATEGORY_TAB_SPECS)
    {
        if visible {
            tabs.push(ToolbarTab::Category(*filter));
        }
    }
    if todo.enabled {
        tabs.push(ToolbarTab::Todos);
    }
    tabs
}

#[cfg(test)]
mod tests {
    use super::{ToolbarTab, visible_toolbar_tabs};
    use clipboard_core::ContentCategory;
    use clipboard_core::preferences::{CategoryTabVisibility, TodoPreference};

    #[test]
    fn todo_visibility_appends_last_when_enabled() {
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
                    enabled: false,
                    ..Default::default()
                }
            ),
            vec![all, image, other]
        );
    }
}
