use super::*;

impl ClipboardView {
    pub(super) fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let item = &self.history.items[index];
        let id = item.id;
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let file_paths = is_files
            .then_some(item.file_paths.as_deref())
            .flatten()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok());
        let file_info = self.file_card_info.get(&id);
        let thumbnail_path = if is_image {
            item.image_path.as_ref().map(PathBuf::from)
        } else {
            file_info.and_then(|info| info.image_path.clone())
        };
        let show_file_icon = is_files && thumbnail_path.is_none();
        let file_warning = file_info.and_then(|info| match info.availability {
            FileCardAvailability::Available => None,
            FileCardAvailability::Missing => {
                Some(tr(self.language, "源文件已失效", "Source file missing"))
            }
            FileCardAvailability::Unreadable => {
                Some(tr(self.language, "文件无法读取", "File unreadable"))
            }
        });
        let file_icon = if file_warning.is_some() {
            IconName::TriangleAlert
        } else if file_info
            .is_some_and(|info| matches!(info.kind, FileCardKind::Folder | FileCardKind::Multiple))
        {
            IconName::Folder
        } else {
            IconName::File
        };
        let image_too_large = file_info.is_some_and(|info| info.image_too_large);
        let image_unavailable = tr(self.language, "无法显示", "Cannot display");
        let kind = match item.content_type.as_str() {
            "image" => tr(self.language, "图片", "Image"),
            "files" => tr(self.language, "文件", "Files"),
            "html" => "HTML",
            "rtf" => "RTF",
            "url" => tr(self.language, "网址", "URL"),
            _ => tr(self.language, "文本", "Text"),
        };
        let pinned = item.is_pinned;
        let summary = self.row_summary(
            item,
            kind,
            file_paths.as_ref().map_or(0, Vec::len),
            file_warning,
            image_too_large,
            file_info.and_then(|info| info.total_size),
        );
        let source_icon = item
            .source_app_icon
            .as_deref()
            .filter(|path| !path.is_empty());
        let (show_source_name, show_source_icon) = if self.display.show_source_app {
            source_app_parts(self.display.source_app_display, source_icon.is_some())
        } else {
            (false, false)
        };
        let selected = !self.batch_mode && self.history.selected == Some(id);
        let marked = self.selected_ids.contains(&id);
        let (preview_text, highlights) = if is_image {
            (String::new(), Vec::new())
        } else {
            let saved_preview = item.preview.as_deref().unwrap_or(kind);
            let search = self.search.read(cx).value();
            let file_names = file_paths
                .as_deref()
                .filter(|paths| paths.len() > 1)
                .map(|paths| {
                    let mut names = paths
                        .iter()
                        .take(3)
                        .map(|path| {
                            Path::new(path)
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.clone())
                        })
                        .collect::<Vec<_>>()
                        .join(if self.language == LanguagePreference::Chinese {
                            "、"
                        } else {
                            ", "
                        });
                    if paths.len() > 3 {
                        names.push('…');
                    }
                    names
                });
            let preview_text = file_names.unwrap_or_else(|| {
                item.text_content
                    .as_deref()
                    .and_then(|full| search_excerpt(full, saved_preview, &search))
                    .unwrap_or_else(|| saved_preview.to_owned())
            });
            let highlights = search_highlight_ranges(&preview_text, &search)
                .into_iter()
                .map(|range| {
                    (
                        range,
                        HighlightStyle {
                            color: Some(cx.theme().primary),
                            background_color: Some(cx.theme().primary.opacity(0.18)),
                            font_weight: Some(FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )
                })
                .collect::<Vec<_>>();
            (preview_text, highlights)
        };
        let drag = HistoryDrag {
            id,
            pinned,
            favorite_only: self.history.favorite_only(),
            group_id: self.history.group_id(),
            generation: self.history.generation,
            kind,
            preview: item.preview.clone().unwrap_or_else(|| kind.to_owned()),
            language: self.language,
        };
        let drag_enabled = !self.reorder_pending && !self.history.loading && !self.batch_mode;
        let show_drag_area_indicator = self.display.show_drag_area_indicator;
        let active_drop = cx.has_active_drag().then_some(self.drop_target).flatten();
        let live_drag_source = cx.has_active_drag() && self.history.selected == Some(id);
        let color = if selected || marked {
            cx.theme().accent
        } else {
            cx.theme().background
        };
        let card_spacing = visual::card_spacing(self.display.card_density);
        let (thumbnail_width, thumbnail_height) = if is_image {
            visual::thumbnail_size(self.display.card_density)
        } else {
            (48., 36.)
        };
        let row = div()
            .id(("history-row", id as usize))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, _| this.cancel_pending_row_click()),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hover_source(id, *hovered, cx);
            }))
            .h(px(history_row_height(item, self.display)))
            .px(px(PAGE_PADDING))
            .py(px(card_spacing))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if cx.has_active_drag() {
                    return;
                }
                this.handle_row_click(id, event, window, cx);
            }))
            .child(
                div()
                    .h_full()
                    .group("")
                    .when(self.batch_mode, |card| card.px_3())
                    .when(!self.batch_mode, |card| card.pl(px(22.)).pr(px(22.)))
                    .py(px(card_spacing))
                    .rounded_md()
                    .border_1()
                    .border_color(match active_drop.filter(|target| target.id == id) {
                        Some(target) if target.allowed => cx.theme().primary,
                        Some(_) => cx.theme().danger,
                        None if selected || marked => cx.theme().primary,
                        None => cx.theme().border,
                    })
                    .bg(color)
                    .relative()
                    .when(active_drop.is_some_and(|target| target.id == id), |card| {
                        card.child(visual::reveal(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .h(px(visual::DROP_MARKER_HEIGHT))
                                .bg(if active_drop.is_some_and(|target| target.allowed) {
                                    cx.theme().primary
                                } else {
                                    cx.theme().danger
                                })
                                .when(active_drop.is_some_and(|target| target.after), |line| {
                                    line.bottom_0()
                                })
                                .when(active_drop.is_some_and(|target| !target.after), |line| {
                                    line.top_0()
                                }),
                            (
                                "drop-marker",
                                id as usize * 2
                                    + usize::from(active_drop.is_some_and(|target| target.after)),
                            ),
                            cx,
                        ))
                    })
                    .flex()
                    .flex_col()
                    .gap(px(card_spacing))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_sm()
                            .when(is_image, |body| body.bg(cx.theme().muted))
                            .px_2()
                            .py(px(card_spacing))
                            .flex()
                            .when(is_image, |body| body.justify_center())
                            .gap_3()
                            .when(show_file_icon, |body| {
                                body.child(
                                    div()
                                        .w(px(thumbnail_width))
                                        .h(px(thumbnail_height))
                                        .flex_none()
                                        .rounded_sm()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(if file_warning.is_some() {
                                            cx.theme().danger.opacity(0.12)
                                        } else {
                                            cx.theme().primary.opacity(0.12)
                                        })
                                        .child(Icon::new(file_icon).large().text_color(
                                            if file_warning.is_some() {
                                                cx.theme().danger
                                            } else {
                                                cx.theme().primary
                                            },
                                        )),
                                )
                            })
                            .when(is_image && thumbnail_path.is_none(), |body| {
                                body.child(image_unavailable)
                            })
                            .when(thumbnail_path.is_some(), |body| {
                                body.child(
                                    div()
                                        .w(px(thumbnail_width))
                                        .h(px(thumbnail_height))
                                        .flex_none()
                                        .rounded_sm()
                                        .overflow_hidden()
                                        .when_some(thumbnail_path, |box_, path| {
                                            box_.child(
                                                img(path)
                                                    .size_full()
                                                    .object_fit(ObjectFit::Contain)
                                                    .with_fallback(move || {
                                                        div()
                                                            .child(image_unavailable)
                                                            .into_any_element()
                                                    }),
                                            )
                                        }),
                                )
                            })
                            .when(!is_image, |body| {
                                body.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_sm()
                                        .text_color(if file_warning.is_some() {
                                            cx.theme().danger
                                        } else {
                                            cx.theme().foreground
                                        })
                                        .line_height(px(20.))
                                        .line_clamp(self.display.card_max_lines as usize)
                                        .text_ellipsis()
                                        .child(
                                            StyledText::new(preview_text)
                                                .with_highlights(highlights),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .h(px(24.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .text_xs()
                            .text_color(if file_warning.is_some() {
                                cx.theme().danger
                            } else {
                                cx.theme().muted_foreground
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(summary),
                            )
                            .when(self.display.show_source_app, |footer| {
                                footer.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .max_w(px(118.))
                                        .overflow_hidden()
                                        .group_hover("", |source| source.invisible())
                                        .when_some(
                                            show_source_icon.then_some(source_icon).flatten(),
                                            |source, path| {
                                                source.child(
                                                    img(PathBuf::from(path))
                                                        .w(px(16.))
                                                        .h(px(16.))
                                                        .flex_none()
                                                        .object_fit(ObjectFit::Contain)
                                                        .with_fallback(|| div().into_any_element()),
                                                )
                                            },
                                        )
                                        .when(
                                            show_source_name
                                                && item
                                                    .source_app_name
                                                    .as_deref()
                                                    .is_some_and(|name| !name.is_empty()),
                                            |source| {
                                                source.child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .text_ellipsis()
                                                        .child(
                                                            item.source_app_name
                                                                .clone()
                                                                .unwrap_or_default(),
                                                        ),
                                                )
                                            },
                                        ),
                                )
                            }),
                    )
                    .when(!self.batch_mode, |card| {
                        card.child(self.render_row_action_bar(id, item, card_spacing, cx))
                    })
                    .when(!self.batch_mode, |card| {
                        card.child(self.drag_handle(
                            id,
                            true,
                            drag.clone(),
                            drag_enabled,
                            show_drag_area_indicator,
                            cx,
                        ))
                        .child(self.drag_handle(
                            id,
                            false,
                            drag.clone(),
                            drag_enabled,
                            show_drag_area_indicator,
                            cx,
                        ))
                    }),
            );
        let row = self.history_row_menu(row, item, cx);
        let row = div()
            .when(live_drag_source, |row| row.opacity(0.0))
            .child(row);
        if let Some(offset) = self
            .reorder_offsets
            .get(&id)
            .filter(|_| !cx.has_active_drag())
        {
            visual::reflow(
                row,
                *offset,
                format!("reorder-feedback-{}-{id}", self.feedback_revision),
                cx,
            )
        } else {
            let (from, to) = self.drag_motion.offsets(index, &self.row_offsets);
            if from != 0. || to != 0. {
                visual::drag_yield(
                    row,
                    from,
                    to,
                    format!("drag-yield-{}-{id}", self.drag_motion.revision),
                    cx,
                )
            } else {
                row.into_any_element()
            }
        }
    }
}
