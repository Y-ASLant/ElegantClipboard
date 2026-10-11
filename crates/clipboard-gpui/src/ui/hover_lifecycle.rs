use super::*;

impl ClipboardView {
    pub(super) fn apply_hover_preview_result(
        &mut self,
        id: i64,
        generation: u64,
        result: Result<PreviewContent, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.hover_preview.apply(id, generation, result)
            && self.hover_source_active
            && self.preview.id.is_none()
        {
            self.open_hover_popup(window, cx);
        }
        true
    }

    pub(super) fn apply_hover_preview_saved(
        &mut self,
        result: Result<HoverPreviewPreference, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.hover_preference_pending = false;
        match result {
            Ok(preference) => {
                self.hover_preference = preference;
                self.close_hover_preview(cx);
                self.message = tr(
                    self.language,
                    "悬停预览设置已保存",
                    "Hover preview preferences saved",
                )
                .into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "悬停预览设置保存失败",
                        "Failed to save hover preview preferences",
                    )
                );
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn close_hover_preview(&mut self, cx: &mut Context<Self>) {
        self.hover_task = None;
        self.hover_close_task = None;
        self.hover_preview.close();
        self.hover_popup_opening = None;
        self.hover_source_active = false;
        self.hover_popup_active = false;
        if let Some(handle) = self.hover_popup.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        cx.notify();
    }

    pub(super) fn schedule_hover_close(&mut self, cx: &mut Context<Self>) {
        self.hover_close_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(180))
                .await;
            let _ = view.update(cx, |this, cx| {
                if !this.hover_source_active && !this.hover_popup_active {
                    this.close_hover_preview(cx);
                }
            });
        }));
    }

    pub(super) fn set_hover_source(&mut self, id: i64, entered: bool, cx: &mut Context<Self>) {
        if entered {
            if self.batch_mode
                || self.history.loading
                || self.preview.id.is_some()
                || self.pending_row_click.is_some()
            {
                return;
            }
            let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
                return;
            };
            if !self.hover_preference.allows(&item.content_type) {
                if self.hover_preview.id.is_some() {
                    self.close_hover_preview(cx);
                }
                return;
            }
            if self.hover_preview.id != Some(id) {
                self.close_hover_preview(cx);
                let generation = self.hover_preview.open(id);
                let delay = self.hover_preference.delay_ms;
                self.hover_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(u64::from(delay)))
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        if this.hover_preview.id == Some(id)
                            && this.hover_preview.generation == generation
                            && this.hover_source_active
                        {
                            this.send(Command::HoverPreview { id, generation }, cx);
                        }
                    });
                }));
            }
            self.hover_source_active = true;
            self.hover_close_task = None;
        } else if self.hover_preview.id == Some(id) {
            self.hover_source_active = false;
            self.schedule_hover_close(cx);
        }
    }

    pub(super) fn set_hover_popup_active(
        &mut self,
        id: i64,
        generation: u64,
        entered: bool,
        cx: &mut Context<Self>,
    ) {
        if self.hover_preview.id != Some(id) || self.hover_preview.generation != generation {
            return;
        }
        self.hover_popup_active = entered;
        if entered {
            self.hover_close_task = None;
        } else if !self.hover_source_active {
            self.schedule_hover_close(cx);
        }
    }

    pub(super) fn save_hover_preference(
        &mut self,
        preference: HoverPreviewPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.hover_preference_pending
            && self.hover_preference != preference
            && self.send(Command::SetHoverPreview(preference), cx)
        {
            self.hover_preference_pending = true;
            cx.notify();
        }
    }

    pub(super) fn open_hover_popup(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.hover_popup.is_some()
            || self.hover_popup_opening.is_some()
            || !self.hover_source_active
        {
            return;
        }
        let (Some(id), Some(result)) = (self.hover_preview.id, self.hover_preview.result.clone())
        else {
            return;
        };
        let generation = self.hover_preview.generation;
        let screen = window.display(cx).map(|display| display.bounds());
        let image_dimensions = (self.hover_preference.expanded_image
            && matches!(&result, Ok(PreviewContent::Image(_))))
        .then(|| {
            self.history
                .items
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| Some((item.image_width?, item.image_height?)))
        })
        .flatten();
        let bounds = position::hover_popup_bounds(
            point(
                window.bounds().origin.x + window.mouse_position().x,
                window.bounds().origin.y + window.mouse_position().y,
            ),
            screen.unwrap_or_else(|| window.bounds()),
            self.hover_preference.position,
            image_dimensions,
        );
        let theme = self.theme;
        let language = self.language;
        let zoom_step = self.hover_preference.zoom_step;
        self.hover_popup_opening = Some((id, generation));
        cx.spawn(async move |owner, cx| {
            let Some(owner_entity) = owner.upgrade() else {
                return;
            };
            let valid = owner_entity.update(cx, |this, _| {
                this.hover_preview.id == Some(id)
                    && this.hover_preview.generation == generation
                    && (this.hover_source_active || this.hover_popup_active)
            });
            if !valid {
                owner_entity.update(cx, |this, _| {
                    if this.hover_popup_opening == Some((id, generation)) {
                        this.hover_popup_opening = None;
                    }
                });
                return;
            }
            let popup_owner = owner.clone();
            let opened = cx.update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: None,
                        kind: WindowKind::PopUp,
                        focus: false,
                        show: true,
                        is_movable: false,
                        is_resizable: false,
                        app_id: Some("com.aslant.elegant-clipboard-gpui.hover".into()),
                        ..Default::default()
                    },
                    cx,
                    move |window, cx| {
                        apply_theme(theme, window, cx);
                        cx.new(|cx| {
                            HoverPreviewWindowView::new(
                                popup_owner.clone(),
                                (id, generation),
                                language,
                                result,
                                zoom_step,
                                window,
                                cx,
                            )
                        })
                    },
                )
            });
            owner_entity.update(cx, |this, cx| {
                if this.hover_popup_opening == Some((id, generation)) {
                    this.hover_popup_opening = None;
                }
                if let Ok((handle, _)) = opened {
                    if this.hover_preview.id == Some(id)
                        && this.hover_preview.generation == generation
                        && (this.hover_source_active || this.hover_popup_active)
                    {
                        this.hover_popup = Some(handle);
                    } else {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        })
        .detach();
    }
}
