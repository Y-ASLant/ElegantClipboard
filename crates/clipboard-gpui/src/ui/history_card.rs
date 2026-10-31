use super::file_cards::{FileCardAvailability, FileCardKind};
use super::presentation::content_type_label;
use super::*;
use clipboard_core::search::search_highlight_ranges;

impl ClipboardView {
    pub(super) fn render_row(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = &self.history.items[index];
        let id = item.id;
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let file_paths = if is_files {
            clipboard_core::parse_file_paths_for_display(item.file_paths.as_deref())
        } else {
            Vec::new()
        };
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
        let kind = content_type_label(self.language, &item.content_type);
        let summary = self.row_summary(
            item,
            kind,
            file_paths.len(),
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
            let file_names = if file_paths.len() > 1 {
                let paths = &file_paths;
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
                Some(names)
            } else {
                None
            };
            let preview_text = file_names.unwrap_or_else(|| saved_preview.to_owned());
            let highlights = search_highlight_ranges(&preview_text, &search)
                .into_iter()
                .map(|range| {
                    (
                        range,
                        HighlightStyle {
                            color: Some(cx.theme().blue),
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
            is_image,
            is_files,
            source_app_name: item.source_app_name.clone(),
            source_app_icon: item.source_app_icon.clone(),
            display: self.display,
            language: self.language,
            summary,
            preview_text,
            highlights,
            thumbnail: thumbnail_path.as_ref().and_then(|path| {
                self.thumbnails
                    .source(&self.thumbnail_key(item, path.clone()))
            }),
            thumbnail_path,
            file_warning: file_warning.is_some(),
            file_icon,
            selected: selected || marked,
        };
        let hovered = window.use_keyed_state(("history-hover", id as usize), cx, |_, _| false);
        let is_hovered = *hovered.read(cx);
        let background = if card.selected {
            cx.theme().list_active
        } else if is_hovered {
            cx.theme().list_hover
        } else {
            cx.theme().colors.list
        };
        let border = if card.selected {
            cx.theme().primary
        } else {
            crate::design_system::container_border(cx)
        };
        let motion = gpui_kit::base::motion::Transition::new(crate::design_system::FAST)
            .easing(cx.theme().motion.easing_move.clone());
        // Translucent color interpolation over the shared material canvas produces
        // additive-alpha fringes. Keep surface changes immediate; actions still fade.
        let surface_motion = if cx.theme().colors.list.a < 1. {
            gpui_kit::base::motion::Transition::new(Duration::ZERO)
        } else {
            motion.clone()
        };
        let surface_channel = if cx.theme().is_dark() {
            "history-surface-dark"
        } else {
            "history-surface-light"
        };
        let border_channel = if cx.theme().is_dark() {
            "history-border-dark"
        } else {
            "history-border-light"
        };
        let background = visual::transition_color(
            ElementId::from((surface_channel, id as usize)),
            background,
            surface_motion.clone(),
            window,
            cx,
        );
        let border = visual::transition_color(
            ElementId::from((border_channel, id as usize)),
            border,
            surface_motion,
            window,
            cx,
        );
        let action_opacity = gpui_kit::base::motion::transition(
            ElementId::from(("history-actions-opacity", id as usize)),
            if is_hovered && !self.card_drag_active(cx) {
                1.0_f32
            } else {
                0.0
            },
            motion,
            window,
            cx,
        );
        let surface = card.surface(false, cx).bg(background).border_color(border);
        let measured_size = Rc::new(Cell::new(size(px(0.), px(0.))));
        let drag_enabled = !self.reorder_pending && !self.history.loading && !self.batch_mode;
        let live_drag_source = self.card_drag_active(cx) && self.drag_source_id() == Some(id);
        let card_spacing = visual::card_spacing(self.display.card_density);
        let card_padding = visual::card_padding(self.display.card_density);
        let entity = cx.entity();
        let row = div()
            .id(("history-row", id as usize))
            .relative()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, _| this.cancel_pending_row_click()),
            )
            .on_hover(cx.listener(move |this, is_hovered: &bool, _, cx| {
                hovered.update(cx, |hovered, _| *hovered = *is_hovered);
                // 鼠标驱动的焦点：悬停进入即转移选中态，与键盘/点击同一目标。
                // 拖拽与批量模式下 selection 不参与渲染与快捷键，保持不变。
                if *is_hovered && !this.batch_mode && !cx.has_active_drag() {
                    this.history.selected = Some(id);
                }
                this.set_hover_source(id, *is_hovered, cx);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                this.update_hover_anchor(id, event.position.y, cx);
            }))
            .on_scroll(cx.listener(move |this, event: &ScrollEvent, window, cx| {
                if event.modifiers.control
                    && this.zoom_hover_from_card(
                        id,
                        event.delta.pixel_delta(window.line_height()).y,
                        cx,
                    )
                {
                    cx.stop_propagation();
                }
            }))
            .h(px(history_row_height(item, self.display)))
            .px(px(PAGE_PADDING))
            .py(px(card_spacing))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                cx.stop_propagation();
                this.close_history_menu(cx);
                if this.card_drag_active(cx) {
                    return;
                }
                this.handle_row_click(id, event, window, cx);
            }))
            .when(drag_enabled, |row| {
                #[cfg(target_os = "linux")]
                let payload_owner = entity.clone();
                let drag = HistoryDrag {
                    id,
                    pinned: item.is_pinned,
                    favorite_only: self.history.favorite_only(),
                    group_id: self.history.group_id(),
                    generation: self.history.generation,
                    card,
                    size: measured_size.clone(),
                    native_preview: false,
                };
                let row = row
                    .cursor_grab()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.close_history_menu(cx);
                            this.prepare_card_drag(id, cx);
                        }),
                    )
                    .on_drag(drag, move |drag, offset, window, cx| {
                        let mut preview = drag.clone();
                        entity.update(cx, |this, cx| {
                            this.cancel_pending_row_click();
                            this.close_hover_preview("card_drag_started", cx);
                            this.history.selected = Some(drag.id);
                            this.drop_target = None;
                            this.drag_motion.hovered = None;
                            this.start_history_drag_scroll(cx);
                            preview.native_preview =
                                this.begin_external_drag(drag, offset, window, cx);
                            cx.notify();
                        });
                        cx.new(|_| preview)
                    });
                #[cfg(target_os = "linux")]
                let row = row.external_drag_payload(move |drag: &HistoryDrag, window, cx| {
                    if native_window::window_id(window).is_some() {
                        return None;
                    }
                    payload_owner.read(cx).wayland_drag_payload(drag)
                });
                row
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
            .child(surface.when(!self.batch_mode, |card| {
                card.child(
                    self.render_row_action_bar(id, item, card_padding, cx)
                        .opacity(action_opacity)
                        .when(action_opacity == 0., |bar| bar.invisible()),
                )
                .child(self.drag_handle(id, true, self.display.show_drag_area_indicator, cx))
                .child(self.drag_handle(
                    id,
                    false,
                    self.display.show_drag_area_indicator,
                    cx,
                ))
            }));
        let row = self.history_row_menu(row, item, cx);
        let row = div()
            .when(live_drag_source, |row| row.opacity(0.))
            .child(row);
        let positioned = if let Some(offset) = self
            .reorder_offsets
            .get(&id)
            .filter(|_| !self.card_drag_active(cx))
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
                    self.drag_motion.started,
                    self.drag_motion.progress.clone(),
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
