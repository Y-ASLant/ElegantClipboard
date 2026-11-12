use super::*;

impl ClipboardView {
    pub(super) fn render_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.onboarding_visible() {
            return self.render_onboarding(window, cx).into_any_element();
        }
        let category_tab_count =
            (self.display.value.category_tabs.enabled_count() + 1).max(1) as f32;
        let group_width = visual::group_select_width(
            f32::from(window.viewport_size().width),
            self.longest_group_label(window, cx),
        );
        // Deduct the group selector, row gap, and the Kit segmented-tab insets.
        let category_width = (window.viewport_size().width
            - px(PAGE_PADDING * 2. + group_width + visual::SPACE_2 * 2. + 6.))
            / category_tab_count;
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
            } else if self.history.category() == ContentCategory::Image {
                tr(self.language, "还没有图片记录", "No image items yet.")
            } else if self.history.category() == ContentCategory::Url {
                tr(self.language, "还没有链接记录", "No link items yet.")
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(ClipboardView::close_history_menu_from_press),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(ClipboardView::close_history_menu_from_press),
            )
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search
                    .update(cx, |search, cx| search.focus(window, cx));
            }))
            .child(
                div()
                    .key_context("HistorySearch")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            this.activate_for_input(window, cx);
                        }),
                    )
                    .flex_none()
                    .px(px(PAGE_PADDING))
                    .pb(px(visual::SPACE_2))
                    .on_action(cx.listener(|this, _: &FocusFirstHistoryItem, window, cx| {
                        this.focus_first_history_item(window, cx);
                    }))
                    .child(
                        Input::new(&self.search)
                            .with_size(gpui_kit::component::Size::Medium)
                            .text_size(px(visual::TEXT_BODY))
                            .rounded(cx.theme().radius)
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).size(px(visual::ICON_SIZE)))
                            .suffix(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(visual::SPACE_2))
                                    .child(
                                        div()
                                            .text_size(px(visual::TEXT_SMALL))
                                            .text_color(cx.theme().muted_foreground)
                                            .child(if self.language == LanguagePreference::English {
                                                if self.todos_open {
                                                    format!("{} to-dos", self.todos.len())
                                                } else {
                                                    format!("{} items", self.history.total)
                                                }
                                            } else {
                                                format!(
                                                    "{} 条记录",
                                                    if self.todos_open {
                                                        self.todos.len() as i64
                                                    } else {
                                                        self.history.total
                                                    }
                                                )
                                            }),
                                    )
                                    .child(
                                        Button::new("history-search-info")
                                            .text()
                                            .icon(Icon::new(IconName::Info).size(px(visual::ICON_SMALL)))
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
                    ),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .w_full()
                    .px(px(PAGE_PADDING))
                    .pb(px(visual::SPACE_2))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_1))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(visual::SPACE_2))
                            .when(self.display.value.show_category_filter, |row| {
                                row.child(
                                    div().flex_1().min_w_0().child(
                                        TabBar::new("history-category-tabs")
                                            .w_full()
                                            .segmented()
                                            .h(px(visual::CONTROL_HEIGHT))
                                            .max_width(category_width)
                                            .last_empty_space(div().w_0())
                                            .selected_index(self.toolbar_tab_index())
                                            .on_click(cx.listener(
                                                |this, index: &usize, window, cx| {
                                                    this.select_toolbar_tab(*index, window, cx);
                                                },
                                            ))
                                            .children(self.toolbar_tabs().into_iter().map(|tab| {
                                                let (label, accessible) = self.toolbar_tab_text(tab);
                                                Tab::new()
                                                    .text_size(px(visual::TEXT_BODY))
                                                    .min_w_0()
                                                    .w(category_width)
                                                    .label(label)
                                                    .aria_label(accessible)
                                                    .flex_1()
                                            })),
                                    ),
                                )
                            })
                            .when(!self.display.value.show_category_filter, |row| {
                                row.child(
                                    Button::new("open-todos")
                                        .h(px(visual::CONTROL_HEIGHT))
                                        .outline()
                                        .selected(self.todos_open)
                                        .label(tr(self.language, "待办", "To-do"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if this.todos_open {
                                                let filter = if this.history.favorite_only() {
                                                    None
                                                } else {
                                                    Some(this.history.category())
                                                };
                                                this.select_category(filter, window, cx);
                                            } else {
                                                this.open_todos(window, cx);
                                            }
                                        })),
                                )
                            })
                            .child(
                                div()
                                    .w(px(group_width))
                                    .flex_none()
                                    .child(self.render_group_select(group_width, window, cx)),
                            ),
                    ),
            )
            .when_some(self.group_move_id, |container, id| {
                container.child(visual::reveal(
                    div().child(
                        div()
                            .px(px(PAGE_PADDING))
                            .pb(px(visual::SPACE_2))
                            .h(px(GROUP_BAR_HEIGHT))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(visual::SPACE_2))
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
                            .child(div().text_size(px(visual::TEXT_SMALL)).child(tr(
                                self.language,
                                "移至",
                                "Move to",
                            )))
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
                        .pb(px(visual::SPACE_2))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(visual::SPACE_2))
                        .child(
                            div()
                                .text_size(px(visual::TEXT_SMALL))
                                .text_color(cx.theme().muted_foreground)
                                .child(if self.language == LanguagePreference::English {
                                    format!("{} selected", self.selected_ids.len())
                                } else {
                                    format!("已选 {} 条", self.selected_ids.len())
                                }),
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
                                .label(
                                    if self.paste_state.target().is_some() && self._tray.is_some() {
                                        tr(self.language, "合并粘贴", "Merge and paste")
                                    } else {
                                        tr(self.language, "合并复制", "Merge and copy")
                                    },
                                )
                                .disabled(
                                    self.batch_pending
                                        || self.paste_state.pending()
                                        || self.selected_ids.len() < 2,
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.merge_selected(window, cx);
                                })),
                        )
                        .when(self.paste_state.sequence_remaining().is_none(), |bar| {
                            bar.child(
                                Button::new("batch-sequence")
                                    .small()
                                    .outline()
                                    .label(tr(
                                        self.language,
                                        if self.sequence_hotkey.trim().is_empty() {
                                            "序列粘贴（推进键已停用）"
                                        } else {
                                            "序列粘贴"
                                        },
                                        if self.sequence_hotkey.trim().is_empty() {
                                            "Sequence paste (advance key off)"
                                        } else {
                                            "Sequence paste"
                                        },
                                    ))
                                    .disabled(
                                        self.batch_pending
                                            || self.paste_state.pending()
                                            || self.selected_ids.len() < 2
                                            || self.paste_state.target().is_none()
                                            || self._tray.is_none()
                                            || self.sequence_hotkey.trim().is_empty(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.start_sequence_paste(window, cx);
                                    })),
                            )
                        })
                        .when_some(self.paste_state.sequence_remaining(), |bar, remaining| {
                            bar.child(
                                div()
                                    .text_size(px(visual::TEXT_SMALL))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if self.language == LanguagePreference::English {
                                        format!(
                                            "Sequence: {} left, advance with {}",
                                            remaining, self.sequence_hotkey
                                        )
                                    } else {
                                        format!(
                                            "序列中：剩 {} 条，按 {} 推进",
                                            remaining, self.sequence_hotkey
                                        )
                                    }),
                            )
                        })
                        .child(
                            Button::new("batch-delete-open")
                                .small()
                                .danger()
                                .label(tr(self.language, "删除选中", "Delete selected"))
                                .disabled(self.batch_pending || self.selected_ids.is_empty())
                                .on_click(cx.listener(|this, _, window, cx| {
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
            .child(if self.todos_open {
                self.render_todos(window, cx).into_any_element()
            } else {
                self.render_history_list(empty_message, cx).into_any_element()
            })
            .into_any_element()
    }
}
