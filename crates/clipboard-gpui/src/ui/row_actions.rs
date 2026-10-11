use super::*;

impl ClipboardView {
    /// 卡片两侧的拖拽把手：除锚定边与元素 id 外逐行一致。
    pub(super) fn drag_handle(
        &self,
        id: i64,
        anchored_left: bool,
        drag: HistoryDrag,
        drag_enabled: bool,
        show_drag_area_indicator: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
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
                    .border_dashed()
                    .border_color(rgb(0x3b82f6))
                    .when(anchored_left, |handle| handle.border_r_1())
                    .when(!anchored_left, |handle| handle.border_l_1())
                    .text_color(rgb(0x3b82f6))
                    .child(Icon::new(gpui_kit::assets::IconName::GripVertical).xsmall())
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.cancel_pending_row_click();
                cx.stop_propagation();
            }))
            .when(drag_enabled, |handle| {
                handle.cursor_grab().on_drag(drag, move |drag, _, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.cancel_pending_row_click();
                        this.close_hover_preview(cx);
                        this.history.selected = Some(drag.id);
                        this.drop_target = None;
                        this.start_history_drag_scroll(cx);
                        cx.notify();
                    });
                    cx.new(|_| drag.clone())
                })
            })
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
        } else if matches!(item.content_type.as_str(), "html" | "rtf") && item.char_count.is_none()
        {
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

    /// 卡片右下角的悬浮操作栏（选择/查看/收藏/置顶/移动/删除/复制等）。
    pub(super) fn render_row_action_bar(
        &self,
        id: i64,
        item: &DbClipboardItem,
        card_spacing: f32,
        cx: &Context<Self>,
    ) -> Div {
        let selected = !self.batch_mode && self.history.selected == Some(id);
        let marked = self.selected_ids.contains(&id);
        let favorite = item.is_favorite;
        let pinned = item.is_pinned;
        div()
            .absolute()
            .bottom(px(card_spacing))
            .right(px(22.))
            .h(px(24.))
            .flex()
            .items_center()
            .gap_1()
            .rounded_sm()
            .bg(cx.theme().background)
            .invisible()
            .group_hover("", |bar| bar.visible())
            .when(selected, |bar| bar.visible())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.cancel_pending_row_click();
                    cx.stop_propagation();
                }),
            )
            .child(
                Button::new(("select", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Check)
                    .tooltip(if marked {
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
                    })
                    .accessibility_label(if marked {
                        tr(self.language, "取消选择", "Deselect")
                    } else {
                        tr(self.language, "选择", "Select")
                    })
                    .selected(marked)
                    .disabled(self.batch_pending || self.history.loading)
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.toggle_selection(id, event.modifiers().shift, window, cx);
                    })),
            )
            .child(
                Button::new(("preview", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Eye)
                    .tooltip(tr(self.language, "查看", "View"))
                    .accessibility_label(tr(self.language, "查看", "View"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_preview(id, window, cx);
                    })),
            )
            .child(
                Button::new(("favorite", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(if favorite {
                        IconName::HeartOff
                    } else {
                        IconName::Heart
                    })
                    .tooltip(if favorite {
                        tr(self.language, "取消收藏", "Unfavorite")
                    } else {
                        tr(self.language, "收藏", "Favorite")
                    })
                    .accessibility_label(if favorite {
                        tr(self.language, "取消收藏", "Unfavorite")
                    } else {
                        tr(self.language, "收藏", "Favorite")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.send(Command::ToggleFavorite(id), cx);
                    })),
            )
            .child(
                Button::new(("pin", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(if pinned {
                        IconName::StarOff
                    } else {
                        IconName::Star
                    })
                    .tooltip(if pinned {
                        tr(self.language, "取消置顶", "Unpin")
                    } else {
                        tr(self.language, "置顶", "Pin")
                    })
                    .accessibility_label(if pinned {
                        tr(self.language, "取消置顶", "Unpin")
                    } else {
                        tr(self.language, "置顶", "Pin")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.send(Command::TogglePin(id), cx);
                    })),
            )
            .child(
                Button::new(("move-group", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Folder)
                    .tooltip(tr(self.language, "移动到分组", "Move to group"))
                    .accessibility_label(tr(self.language, "移动到分组", "Move to group"))
                    .disabled(
                        self.group_move_pending
                            || (self.groups.is_empty() && self.history.group_id().is_none()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.group_move_id = Some(id);
                        this.history.selected = Some(id);
                        cx.notify();
                    })),
            )
            .child(
                Button::new(("delete", id as usize))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Delete)
                    .tooltip(tr(self.language, "删除", "Delete"))
                    .accessibility_label(tr(self.language, "删除", "Delete"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.send(Command::Delete(id), cx);
                    })),
            )
            .when(item.content_type == "files", |bar| {
                bar.child(
                    Button::new(("copy-path", id as usize))
                        .ghost()
                        .xsmall()
                        .h(px(24.))
                        .icon(IconName::Copy)
                        .tooltip(if self.paste_target.is_some() && self._tray.is_some() {
                            tr(self.language, "粘贴路径", "Paste paths")
                        } else {
                            tr(self.language, "复制路径", "Copy paths")
                        })
                        .accessibility_label(
                            if self.paste_target.is_some() && self._tray.is_some() {
                                tr(self.language, "粘贴路径", "Paste paths")
                            } else {
                                tr(self.language, "复制路径", "Copy paths")
                            },
                        )
                        .disabled(self.paste_pending.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.copy_or_paste_path(id, window, cx);
                        })),
                )
            })
            .child(
                Button::new(("paste", id as usize))
                    .outline()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Replace)
                    .tooltip(tr(self.language, "粘贴", "Paste"))
                    .accessibility_label(tr(self.language, "粘贴", "Paste"))
                    .disabled(
                        self.paste_target.is_none()
                            || self.paste_pending.is_some()
                            || self._tray.is_none(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.paste_selected(id, window, cx);
                    })),
            )
            .when(
                matches!(item.content_type.as_str(), "html" | "rtf"),
                |bar| {
                    bar.child(
                        Button::new(("copy-plain", id as usize))
                            .outline()
                            .xsmall()
                            .h(px(24.))
                            .icon(IconName::FileText)
                            .tooltip(if self.paste_target.is_some() {
                                tr(self.language, "粘贴纯文本", "Paste plain text")
                            } else {
                                tr(self.language, "复制纯文本", "Copy plain text")
                            })
                            .accessibility_label(if self.paste_target.is_some() {
                                tr(self.language, "粘贴纯文本", "Paste plain text")
                            } else {
                                tr(self.language, "复制纯文本", "Copy plain text")
                            })
                            .disabled(self.paste_pending.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.copy_or_paste_plain_text(id, window, cx);
                            })),
                    )
                },
            )
            .child(
                Button::new(("copy", id as usize))
                    .outline()
                    .xsmall()
                    .h(px(24.))
                    .icon(IconName::Copy)
                    .tooltip(tr(self.language, "复制", "Copy"))
                    .accessibility_label(tr(self.language, "复制", "Copy"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.send(Command::Copy(id), cx);
                    })),
            )
    }
}
