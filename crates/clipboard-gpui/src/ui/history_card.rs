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
        let kind = match item.content_type.as_str() {
            "image" => tr(self.language, "图片", "Image"),
            "files" => tr(self.language, "文件", "Files"),
            "html" => "HTML",
            "rtf" => "RTF",
            "url" => tr(self.language, "网址", "URL"),
            _ => tr(self.language, "文本", "Text"),
        };
        let summary = self.row_summary(
            item,
            kind,
            file_paths.as_ref().map_or(0, Vec::len),
            file_warning,
            image_too_large,
            file_info.and_then(|info| info.total_size),
        );
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
        let card = drag_visual::HistoryCardSnapshot {
            item: item.clone(),
            display: self.display,
            language: self.language,
            summary,
            preview_text,
            highlights,
            thumbnail_path,
            file_warning: file_warning.is_some(),
            file_icon,
            selected: selected || marked,
            batch_mode: self.batch_mode,
        };
        let measured_size = Rc::new(Cell::new(size(px(0.), px(0.))));
        let drag = HistoryDrag {
            id,
            pinned: item.is_pinned,
            favorite_only: self.history.favorite_only(),
            group_id: self.history.group_id(),
            generation: self.history.generation,
            card: card.clone(),
            size: measured_size.clone(),
            native_preview: false,
        };
        let drag_enabled = !self.reorder_pending && !self.history.loading && !self.batch_mode;
        let live_drag_source = cx.has_active_drag() && self.drag_source_id() == Some(id);
        let card_spacing = visual::card_spacing(self.display.card_density);
        let entity = cx.entity();
        let row = div()
            .id(("history-row", id as usize))
            .relative()
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
            .when(drag_enabled, |row| {
                row.cursor_grab()
                    .on_drag(drag, move |drag, offset, window, cx| {
                        let mut preview = drag.clone();
                        entity.update(cx, |this, cx| {
                            this.cancel_pending_row_click();
                            this.close_hover_preview(cx);
                            this.history.selected = Some(drag.id);
                            this.drop_target = None;
                            this.drag_motion.hovered = None;
                            this.start_history_drag_scroll(cx);
                            preview.native_preview =
                                this.begin_external_drag(drag, offset, window, cx);
                            cx.notify();
                        });
                        cx.new(|_| preview)
                    })
            })
            .child(
                canvas(
                    move |bounds, _, _| measured_size.set(bounds.size),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(card.surface(false, cx).when(!self.batch_mode, |card| {
                card.child(self.render_row_action_bar(id, item, card_spacing, cx))
                    .child(self.drag_handle(id, true, self.display.show_drag_area_indicator, cx))
                    .child(self.drag_handle(id, false, self.display.show_drag_area_indicator, cx))
            }));
        let row = self.history_row_menu(row, item, cx);
        let row = div()
            .when(live_drag_source, |row| row.opacity(0.))
            .child(row);
        let positioned = if let Some(offset) = self
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
            if live_drag_source {
                row.relative().top(px(to)).into_any_element()
            } else if from != 0. || to != 0. {
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
        };
        // VirtualList assigns its row root a fixed origin via prepaint_at. Move a child
        // within that root so relative offsets survive the list's positioning.
        div()
            .w_full()
            .h(px(history_row_height(item, self.display)))
            .child(positioned)
            .into_any_element()
    }
}
