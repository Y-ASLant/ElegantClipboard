use super::*;

impl ClipboardView {
    pub(super) fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.onboarding_visible() {
            return self.render_onboarding(cx).into_any_element();
        }
        if self.preview.id.is_some() {
            return visual::reveal(
                self.render_preview(cx),
                ("preview-enter", self.preview.generation as usize),
                cx,
            );
        }
        let empty_message = if self.history.loading {
            tr(self.language, "正在加载…", "Loading…")
        } else if self.search.read(cx).value().is_empty() {
            if self.history.favorite_only() {
                tr(
                    self.language,
                    "还没有收藏，点击记录上的“收藏”保留常用文本",
                    "No favorites yet. Mark an item as a favorite to keep it handy.",
                )
            } else if self.history.group_id().is_some() {
                tr(
                    self.language,
                    "该分组暂无可显示的记录",
                    "There are no items in this group.",
                )
            } else if self.history.category() == ContentCategory::Text {
                tr(self.language, "还没有文本记录", "No text items yet.")
            } else if self.history.category() == ContentCategory::Other {
                tr(self.language, "还没有其他类型记录", "No other items yet.")
            } else {
                tr(
                    self.language,
                    "复制一段文本，它会出现在这里",
                    "Copy something and it will appear here.",
                )
            }
        } else {
            tr(
                self.language,
                "没有匹配的记录，试试其他关键词",
                "No matching items. Try another search.",
            )
        };
        div()
            .key_context("ClipboardApp")
            .flex()
            .flex_col()
            .size_full()
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                this.dismiss_or_hide(window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .child(
                div()
                    .key_context("HistorySearch")
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .on_action(cx.listener(|this, _: &FocusFirstHistoryItem, window, cx| {
                        this.focus_first_history_item(window, cx);
                    }))
                    .child(
                        Input::new(&self.search)
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).small())
                            .suffix(
                                Button::new("history-search-info")
                                    .text()
                                    .icon(IconName::Info)
                                    .xsmall()
                                    .accessibility_label(tr(
                                        self.language,
                                        "搜索说明",
                                        "Search help",
                                    ))
                                    .tooltip(tr(
                                        self.language,
                                        "输入关键词搜索剪贴板历史；点击 × 清空搜索。",
                                        "Search clipboard history by keyword; click × to clear.",
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .w_full()
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(self.display.show_category_filter, |row| {
                                let selected_index = self
                                    .history
                                    .group_id()
                                    .is_none()
                                    .then(|| self.category_tab_index());
                                row.child(
                                    div().flex_1().min_w_0().child(
                                        TabBar::new("history-category-tabs")
                                            .w_full()
                                            .segmented()
                                            .when_some(selected_index, |tabs, index| {
                                                tabs.selected_index(index)
                                            })
                                            .on_click(cx.listener(
                                                |this, index: &usize, window, cx| {
                                                    if let Some(&category) =
                                                        CATEGORY_FILTERS.get(*index)
                                                    {
                                                        this.select_category(category, window, cx);
                                                    }
                                                },
                                            ))
                                            .children(
                                                [
                                                    ("全部", "All", "All"),
                                                    ("收藏", "Favs", "Favorites"),
                                                    ("文本", "Text", "Text"),
                                                    ("其他", "Other", "Other"),
                                                ]
                                                .map(|(chinese, english, accessible)| {
                                                    Tab::new()
                                                        .label(tr(self.language, chinese, english))
                                                        .aria_label(tr(
                                                            self.language,
                                                            chinese,
                                                            accessible,
                                                        ))
                                                        .flex_1()
                                                }),
                                            ),
                                    ),
                                )
                            })
                            .child(
                                div().w(px(138.)).flex_none().child(
                                    Select::new(&self.group_select)
                                        .w_full()
                                        .menu_width(px(190.))
                                        .menu_max_h(px(300.))
                                        .placeholder(tr(self.language, "选择分组", "Select group"))
                                        .accessibility_label(tr(self.language, "分组", "Group"))
                                        .disabled(
                                            self.group_save_pending
                                                || self.group_delete_pending
                                                || self.clear_pending
                                                || self.group_reorder_pending,
                                        ),
                                ),
                            ),
                    ),
            )
            .when_some(self.group_move_id, |container, id| {
                container.child(visual::reveal(
                    div().child(
                        div()
                            .px(px(PAGE_PADDING))
                            .pb_2()
                            .h(px(GROUP_BAR_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap_2()
                            .overflow_x_scrollbar()
                            .child(
                                Button::new("move-group-cancel")
                                    .small()
                                    .ghost()
                                    .label(tr(self.language, "取消移动", "Cancel move"))
                                    .disabled(self.group_move_pending)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.group_move_id = None;
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_xs().child(tr(self.language, "移至", "Move to")))
                            .child(
                                Button::new("move-group-default")
                                    .small()
                                    .outline()
                                    .label(tr(self.language, "默认分组", "Default"))
                                    .disabled(
                                        self.group_move_pending
                                            || self.history.group_id().is_none(),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_to_group(id, None, cx);
                                    })),
                            )
                            .children(self.groups.iter().map(|group| {
                                let target = Some(group.id);
                                Button::new(("move-group", group.id as usize))
                                    .small()
                                    .outline()
                                    .label(group.name.clone())
                                    .disabled(
                                        self.group_move_pending
                                            || self.history.group_id() == target,
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_to_group(id, target, cx);
                                    }))
                            })),
                    ),
                    ("group-move", id as usize),
                    cx,
                ))
            })
            .when(self.batch_mode, |container| {
                container.child(visual::reveal(
                    div()
                        .px(px(PAGE_PADDING))
                        .pb_2()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(if self.language == LanguagePreference::English {
                                    format!("{} selected", self.selected_ids.len())
                                } else {
                                    format!("已选 {} 条", self.selected_ids.len())
                                }),
                        )
                        .child(
                            Button::new("batch-select-loaded")
                                .small()
                                .ghost()
                                .label(tr(self.language, "全选已加载", "Select loaded"))
                                .disabled(
                                    self.batch_pending
                                        || self.history.loading
                                        || self.selected_ids.len() == self.history.items.len(),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.select_all_loaded(cx))),
                        )
                        .child(
                            Button::new("batch-exit")
                                .small()
                                .ghost()
                                .label(tr(self.language, "退出多选", "Exit selection"))
                                .disabled(self.batch_pending)
                                .on_click(cx.listener(|this, _, _, cx| this.exit_batch_mode(cx))),
                        )
                        .child(
                            Button::new("batch-merge")
                                .small()
                                .outline()
                                .label(if self.paste_target.is_some() && self._tray.is_some() {
                                    tr(self.language, "合并粘贴", "Merge and paste")
                                } else {
                                    tr(self.language, "合并复制", "Merge and copy")
                                })
                                .disabled(
                                    self.batch_pending
                                        || self.paste_pending.is_some()
                                        || self.selected_ids.len() < 2,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.merge_selected(window, cx);
                                })),
                        )
                        .child(
                            Button::new("batch-delete-open")
                                .small()
                                .danger()
                                .label(tr(self.language, "删除选中", "Delete selected"))
                                .disabled(self.batch_pending || self.selected_ids.is_empty())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.batch_confirm_open = true;
                                    this.clear_confirm_open = false;
                                    this.group_delete_id = None;
                                    this.open_history_confirmation(
                                        HistoryConfirmation::DeleteSelected,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                    (
                        "batch-toolbar",
                        self.history.group_id().unwrap_or(0) as usize,
                    ),
                    cx,
                ))
            })
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex_1()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .window_control_area(WindowControlArea::Drag)
                            .child(if self.language == LanguagePreference::English {
                                format!("{} items", self.history.total)
                            } else {
                                format!("{} 条记录", self.history.total)
                            }),
                    ),
            )
            .child(self.render_history_list(empty_message, cx))
            .when(
                (self.history.items.len() as i64) < self.history.total
                    && self.history.limit < HISTORY_LIMIT,
                |container| {
                    container.child(
                        div().px(px(PAGE_PADDING)).py_2().child(
                            Button::new("more")
                                .ghost()
                                .small()
                                .label(tr(self.language, "加载更多", "Load more"))
                                .disabled(self.history.loading)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history.limit =
                                        (this.history.limit + PAGE_SIZE).min(HISTORY_LIMIT);
                                    this.history.generation += 1;
                                    this.history.loading = true;
                                    this.query(cx);
                                    cx.notify();
                                })),
                        ),
                    )
                },
            )
            .into_any_element()
    }
}
