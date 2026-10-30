use super::*;

impl ClipboardView {
    /// 「移动到分组」是否可用：有待处理移动、或无分组且在默认分组时禁用。
    pub(super) fn group_move_disabled(&self) -> bool {
        self.group_move_pending || (self.groups.is_empty() && self.history.group_id().is_none())
    }

    /// 两侧仅显示拖拽提示；整张卡片负责拖拽手势。
    pub(super) fn drag_handle(
        &self,
        id: i64,
        anchored_left: bool,
        show_drag_area_indicator: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let mut handle = div()
            .id((
                if anchored_left {
                    "history-drag-left"
                } else {
                    "history-drag-right"
                },
                id as usize,
            ))
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(18.))
            .flex()
            .items_center()
            .justify_center();
        handle = if anchored_left {
            handle.left_0()
        } else {
            handle.right_0()
        };
        handle
            .when(show_drag_area_indicator, |handle| {
                handle
                    .invisible()
                    .group_hover("", |handle| handle.visible())
                    .text_color(cx.theme().primary)
                    .child(
                        Icon::new(gpui_kit::assets::IconName::GripVertical)
                            .size(px(visual::ICON_SMALL)),
                    )
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.cancel_pending_row_click();
                cx.stop_propagation();
            }))
    }

    /// 卡片摘要行文本：时间/类型、置顶标记、字符数、大小与失效提示。
    pub(super) fn row_summary(
        &self,
        item: &DbClipboardItem,
        kind: &'static str,
        file_count: usize,
        file_warning: Option<&'static str>,
        image_too_large: bool,
        file_total_size: Option<u64>,
    ) -> String {
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let detail = if is_image {
            None
        } else if is_files {
            if file_count == 0 {
                Some(tr(self.language, "路径不可用", "Paths unavailable").into())
            } else if self.language == LanguagePreference::English {
                Some(format!("{file_count} items"))
            } else {
                Some(format!("{file_count} 项"))
            }
        } else if !self.display.show_char_count {
            None
        } else if item.kind().is_some_and(|kind| kind.is_rich()) && item.char_count.is_none() {
            Some(tr(self.language, "纯文本未知", "Plain text unavailable").into())
        } else if self.language == LanguagePreference::English {
            Some(format!("{} characters", item.char_count.unwrap_or(0)))
        } else {
            Some(format!("{} 字符", item.char_count.unwrap_or(0)))
        };
        let mut summary = if self.display.show_time {
            format_card_time(
                &item.created_at,
                self.display.time_format,
                self.language,
                chrono::Local::now().naive_local(),
            )
        } else {
            kind.to_owned()
        };
        if item.is_pinned {
            summary.push_str(tr(self.language, " · 置顶", " · Pinned"));
        }
        if self.display.show_time
            && !is_image
            && !matches!(item.content_type.as_str(), "text" | "url")
        {
            summary.push_str(" · ");
            summary.push_str(kind);
        }
        if let Some(detail) = detail {
            summary.push_str(" · ");
            summary.push_str(&detail);
        }
        if self.display.show_byte_size {
            let size = if is_files {
                file_total_size
            } else {
                Some(item.byte_size.max(0) as u64)
            };
            if let Some(size) = size {
                summary.push_str(" · ");
                summary.push_str(&format_bytes(size));
            }
        }
        if let Some(warning) = file_warning {
            summary.push_str(" · ");
            summary.push_str(warning);
        } else if image_too_large {
            summary.push_str(" · ");
            summary.push_str(tr(self.language, "图片过大", "Image too large"));
        }
        summary
    }

    /// 卡片操作栏的公共按钮构造：ghost 小图标按钮，
    /// tooltip 与无障碍标签共用同一文案，点击时拦截冒泡。
    #[allow(clippy::too_many_arguments)]
    fn action_button(
        &self,
        name: &'static str,
        id: i64,
        icon: impl Into<Icon>,
        label: impl Into<SharedString>,
        disabled: bool,
        selected: bool,
        cx: &Context<Self>,
        on_click: impl Fn(&mut Self, &ClickEvent, &mut Window, &mut Context<Self>) + 'static,
    ) -> Button {
        let label: SharedString = label.into();
        Button::new((name, id as usize))
            .ghost()
            .xsmall()
            .h(px(visual::SPACE_6))
            .icon(icon.into().size(px(visual::ICON_SMALL)))
            .tooltip(label.clone())
            .accessibility_label(label)
            .selected(selected)
            .disabled(disabled)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                on_click(this, event, window, cx);
            }))
    }

    /// 卡片右上角的管理操作栏（选择/收藏/置顶/移动/删除）。
    pub(super) fn render_row_action_bar(
        &self,
        id: i64,
        item: &DbClipboardItem,
        card_padding: f32,
        cx: &Context<Self>,
    ) -> Div {
        let marked = self.selected_ids.contains(&id);
        let favorite = item.is_favorite;
        let pinned = item.is_pinned;
        div()
            .absolute()
            .top(px(card_padding))
            .right(px(card_padding))
            .h(px(visual::SPACE_6 + visual::SPACE_1))
            .px(px(visual::SPACE_1 / 2.))
            .flex()
            .items_center()
            .gap(px(visual::SPACE_1))
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_color(cx.theme().popover_foreground)
            .border(px(visual::BORDER_WIDTH))
            .border_color(cx.theme().border)
            .shadow(visual::control_shadow())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.cancel_pending_row_click();
                    cx.stop_propagation();
                }),
            )
            .child(self.action_button(
                "select",
                id,
                IconName::Check,
                if marked {
                    tr(
                        self.language,
                        "取消选择；Shift 点击可连选",
                        "Deselect; Shift-click to select a range",
                    )
                } else {
                    tr(
                        self.language,
                        "选择；Shift 点击可连选",
                        "Select; Shift-click to select a range",
                    )
                },
                self.batch_pending || self.history.loading,
                marked,
                cx,
                move |this, event, window, cx| {
                    this.toggle_selection(id, event.modifiers().shift, window, cx);
                },
            ))
            .child(self.action_button(
                "favorite",
                id,
                if favorite {
                    IconName::HeartOff
                } else {
                    IconName::Heart
                },
                if favorite {
                    tr(self.language, "取消收藏", "Unfavorite")
                } else {
                    tr(self.language, "收藏", "Favorite")
                },
                false,
                false,
                cx,
                move |this, _, _, cx| {
                    this.send(Command::ToggleFavorite(id), cx);
                },
            ))
            .child(self.action_button(
                "pin",
                id,
                if pinned {
                    IconName::StarOff
                } else {
                    IconName::Star
                },
                if pinned {
                    tr(self.language, "取消置顶", "Unpin")
                } else {
                    tr(self.language, "置顶", "Pin")
                },
                false,
                false,
                cx,
                move |this, _, _, cx| {
                    this.send(Command::TogglePin(id), cx);
                },
            ))
            .child(self.action_button(
                "move-group",
                id,
                IconName::Folder,
                tr(self.language, "移动到分组", "Move to group"),
                self.group_move_disabled(),
                false,
                cx,
                move |this, _, _, cx| {
                    this.group_move_id = Some(id);
                    this.history.selected = Some(id);
                    cx.notify();
                },
            ))
            .child(self.action_button(
                "delete",
                id,
                gpui_kit::assets::IconName::Trash,
                tr(self.language, "删除", "Delete"),
                false,
                false,
                cx,
                move |this, _, _, cx| {
                    this.send(Command::Delete(id), cx);
                },
            ))
    }
}
