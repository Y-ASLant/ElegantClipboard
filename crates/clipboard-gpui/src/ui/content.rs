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
        let chrome = self.list_chrome.clone();
        // GPUI's cached replay omits semantic nodes; rebuild for connected accessibility clients.
        let cache_subtrees = !window.is_a11y_active();
        let height = chrome.read(cx).height.get().filter(|_| cache_subtrees);
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
                this.focus_search_for_input(window, cx);
            }))
            .child(if let Some(height) = height {
                chrome
                    .cached(StyleRefinement::default().w_full().h(height))
                    .into_any_element()
            } else {
                chrome.into_any_element()
            })
            .child(div().flex_1().min_h_0().child(if cache_subtrees {
                self.list_surface
                    .clone()
                    .cached(StyleRefinement::default().size_full())
                    .into_any_element()
            } else {
                self.list_surface.clone().into_any_element()
            }))
            .into_any_element()
    }

    pub(super) fn render_list_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search = Input::new(&self.search)
            .with_size(gpui_kit::component::Size::Medium)
            .text_size(px(visual::TEXT_BODY))
            .rounded(cx.theme().radius)
            .cleanable(true)
            .prefix(Icon::new(IconName::Search).size(px(visual::ICON_SIZE)))
            .suffix(self.render_search_status(window, cx));
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(
                div()
                    .px(px(PAGE_PADDING))
                    .pb(px(visual::SPACE_2))
                    .flex_none()
                    .child(
                        div()
                            .key_context("HistorySearch")
                            .on_action(cx.listener(
                                |this, _: &FocusFirstHistoryItem, window, cx| {
                                    this.focus_first_history_item(window, cx);
                                },
                            ))
                            // Run after Kit places the caret. Its frame can take focus
                            // without focusing the editor when an addon is clicked.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                    this.focus_search_for_input(window, cx);
                                }),
                            )
                            .child(Styled::h(search, px(visual::CONTROL_HEIGHT))),
                    ),
            )
            .child(
                div()
                    .id("toolbar-row")
                    .w_full()
                    .px(px(PAGE_PADDING))
                    .pb(px(visual::SPACE_2))
                    .flex_none()
                    // One chrome slot: filter tabs, or batch / move / locate — never stacked.
                    .child(self.render_toolbar_slot(window, cx)),
            )
            .into_any_element()
    }

    /// Filter row or a transient mode strip. Modes replace tabs; they do not stack under them.
    fn render_toolbar_slot(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.batch_mode {
            return visual::reveal(
                self.render_batch_strip(window, cx),
                (
                    "batch-toolbar",
                    self.history.group_id().unwrap_or(0) as usize,
                ),
                cx,
            );
        }
        if let Some(id) = self.group_move_id {
            return visual::reveal(
                self.render_move_strip(id, cx),
                ("group-move", id as usize),
                cx,
            );
        }
        if let Some((position, total)) = self.locate_progress().filter(|(_, total)| *total > 1) {
            return self
                .render_locate_strip(position, total, cx)
                .into_any_element();
        }
        self.render_filter_toolbar(window, cx).into_any_element()
    }

    fn render_filter_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let group = (!self.todos_open).then(|| self.render_group_select(window, cx));
        let mut category_space = window.viewport_size().width - px(PAGE_PADDING * 2.);
        if let Some((width, _)) = &group {
            category_space -= *width + px(visual::SPACE_2);
        }
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(visual::SPACE_2))
            .when(self.display.value.show_category_filter, |row| {
                row.child(self.render_category_filter(category_space, window, cx))
            })
            .when(
                !self.display.value.show_category_filter && self.todo_preference.value.enabled,
                |row| {
                    row.child(
                        Button::new("open-todos")
                            .small()
                            .h(px(visual::CONTROL_HEIGHT))
                            .outline()
                            .selected(self.todos_open)
                            .label(tr(self.language, "备忘录", "Memo"))
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
                },
            )
            .when_some(group, |row, (width, trigger)| {
                row.child(div().ml_auto().w(width).flex_none().child(trigger))
            })
    }

    fn render_batch_strip(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let can_merge_or_sequence =
            self.selected_ids.len() >= 2 && !self.batch_pending && !self.paste_state.pending();
        let sequence_hotkey_off = self.sequence_hotkey.trim().is_empty();
        let sequence_ready = can_merge_or_sequence
            && self.paste_state.target().is_some()
            && self._tray.is_some()
            && !sequence_hotkey_off;
        let merge_label = if self.paste_state.target().is_some() && self._tray.is_some() {
            tr(self.language, "合并粘贴", "Merge paste")
        } else {
            tr(self.language, "合并复制", "Merge copy")
        };
        let sequence_hint = if sequence_hotkey_off {
            tr(
                self.language,
                "序列粘贴需要先在快捷键设置中启用推进键",
                "Set an advance hotkey in Shortcuts to use sequence paste",
            )
        } else {
            tr(self.language, "序列粘贴", "Sequence paste")
        };
        let sequence_status = self.paste_state.sequence_remaining().map(|remaining| {
            if self.language == LanguagePreference::English {
                format!("{remaining} left · {}", self.sequence_hotkey)
            } else {
                format!("剩 {remaining} · {}", self.sequence_hotkey)
            }
        });
        let button_height = visual::CONTROL_HEIGHT - 2. * (visual::SPACE_1 + visual::BORDER_WIDTH);
        // Keep room for both four-character labels at the minimum window width.
        let (button_min_width, button_padding) = if window.viewport_size().width < px(400.) {
            (visual::CONTROL_HEIGHT + visual::SPACE_3, visual::SPACE_1)
        } else {
            (
                visual::CONTROL_HEIGHT + 2. * visual::SPACE_3,
                visual::SPACE_2,
            )
        };
        let button = |id, label: &'static str| {
            Button::new(id)
                .small()
                .h(px(button_height))
                .min_w(px(button_min_width))
                .px(px(button_padding))
                .accessibility_label(label)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(visual::TEXT_BODY))
                        .child(label),
                )
        };
        let exit = button("batch-exit", tr(self.language, "退出", "Exit"))
            .outline()
            .tooltip(tr(self.language, "退出多选", "Exit multi-select"))
            .disabled(self.batch_pending)
            .on_click(cx.listener(|this, _, _, cx| this.exit_batch_mode(cx)));
        let invert = button(
            "batch-invert-selection",
            tr(self.language, "反选", "Invert"),
        )
        .ghost()
        .tooltip(tr(
            self.language,
            "反选当前列表勾选",
            "Invert selection on the loaded list",
        ))
        .disabled(self.batch_pending || self.history.items.is_empty())
        .on_click(cx.listener(|this, _, _, cx| this.invert_batch_selection(cx)));
        let actions = div()
            .ml_auto()
            .flex()
            .min_w_0()
            .items_center()
            .justify_end()
            .gap(px(visual::SPACE_1))
            .child(
                button("batch-merge", merge_label)
                    .flex_shrink_1()
                    .outline()
                    .tooltip(merge_label)
                    .disabled(!can_merge_or_sequence)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.merge_selected(window, cx);
                    })),
            )
            .when(sequence_status.is_none(), |row| {
                row.child(
                    button("batch-sequence", tr(self.language, "序列粘贴", "Sequence"))
                        .flex_shrink_1()
                        .outline()
                        .tooltip(sequence_hint)
                        .disabled(!sequence_ready)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_sequence_paste(window, cx);
                        })),
                )
            })
            .child(
                button("batch-delete-open", tr(self.language, "删除", "Delete"))
                    .danger()
                    .disabled(self.batch_pending || self.selected_ids.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_history_confirmation(
                            HistoryConfirmation::DeleteSelected,
                            window,
                            cx,
                        );
                    })),
            );
        let mut strip = Self::mode_chrome_strip(cx)
            .p(px(visual::SPACE_1))
            .h(px(visual::CONTROL_HEIGHT))
            .gap(px(visual::SPACE_1))
            .child(exit)
            .child(invert);
        if let Some(status) = sequence_status {
            strip = strip.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(visual::TEXT_SMALL))
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            );
        }
        strip.child(actions)
    }

    fn render_move_strip(&self, id: i64, cx: &mut Context<Self>) -> Div {
        Self::mode_chrome_strip(cx).child(
            div().flex_1().min_w_0().overflow_x_scrollbar().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(visual::SPACE_2))
                    .child(
                        Button::new("move-group-cancel")
                            .small()
                            .ghost()
                            .label(tr(self.language, "取消", "Cancel"))
                            .disabled(self.group_move_pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.group_move_id = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(visual::TEXT_SMALL))
                            .text_color(cx.theme().muted_foreground)
                            .child(tr(self.language, "移至", "Move to")),
                    )
                    .child(
                        Button::new("move-group-default")
                            .small()
                            .outline()
                            .label(tr(self.language, "默认分组", "Default"))
                            .disabled(self.group_move_pending || self.history.group_id().is_none())
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
                            .disabled(self.group_move_pending || self.history.group_id() == target)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.move_to_group(id, target, cx);
                            }))
                    })),
            ),
        )
    }

    fn render_locate_strip(&self, position: usize, total: usize, cx: &mut Context<Self>) -> Div {
        Self::mode_chrome_strip(cx)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(visual::TEXT_SMALL))
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.language == LanguagePreference::English {
                        format!("Match {position} of {total}")
                    } else {
                        format!("匹配 {position} / {total}")
                    }),
            )
            .child(
                Button::new("locate-previous")
                    .small()
                    .outline()
                    .label(tr(self.language, "上一个", "Previous"))
                    .disabled(position <= 1)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.locate_step(-1, cx);
                    })),
            )
            .child(
                Button::new("locate-next")
                    .small()
                    .outline()
                    .label(tr(self.language, "下一个", "Next"))
                    .disabled(position >= total)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.locate_step(1, cx);
                    })),
            )
    }

    fn mode_chrome_strip(cx: &App) -> Div {
        div()
            .w_full()
            .min_h(px(visual::CONTROL_HEIGHT))
            .px(px(visual::SPACE_2))
            .py(px(visual::SPACE_1))
            .rounded(cx.theme().radius)
            .bg(cx.theme().muted)
            .border_1()
            .border_color(crate::design_system::container_border(cx))
            .flex()
            .items_center()
            .gap(px(visual::SPACE_2))
    }
}
