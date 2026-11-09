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
        tracing::debug!(
            id, generation, current_id = ?self.hover_preview.id,
            current_generation = self.hover_preview.generation, success = result.is_ok(),
            source_active = self.hover_source_active, "Hover preview result received"
        );
        if result.is_err() {
            tracing::warn!(
                id,
                generation,
                "Hover preview backend request failed; details shown in popup"
            );
        }
        if self.hover_preview.apply(id, generation, result)
            && self.hover_source_active
            && self.editor.id.is_none()
            && self.hover_source_allowed(window)
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
                self.close_hover_preview("preferences_saved", cx);
                self.is_error = false;
            }
            Err(error) => {
                tracing::error!(%error, "Saving hover preview preferences failed");
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "悬停预览设置保存失败",
                        "Failed to save hover preview preferences",
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn close_hover_preview(&mut self, reason: &'static str, cx: &mut Context<Self>) {
        let id = self.hover_preview.id;
        let generation = self.hover_preview.generation;
        if id.is_some() || self.hover_popup.is_some() || self.hover_popup_opening.is_some() {
            tracing::debug!(
                reason, ?id, generation, opening = ?self.hover_popup_opening,
                source_active = self.hover_source_active,
                window_id = ?self.hover_popup.map(|handle| handle.window_id()), "Closing hover preview"
            );
        }
        self.hover_task = None;
        self.hover_preview.close();
        self.hover_popup_opening = None;
        self.hover_source_active = false;
        if let Some(handle) = self.hover_popup.take() {
            if let Err(error) = handle.update(cx, |_, window, cx| {
                window_lifecycle::log_window_geometry(window, cx, "hover_close_requested");
                window.remove_window();
            }) {
                tracing::warn!(?id, generation, reason, %error, "Closing hover window failed");
            } else {
                tracing::debug!(?id, generation, reason, "Hover window removed");
            }
        }
        cx.notify();
    }

    /// Recover missed source-card leave events without making the passive overlay
    /// itself a hover target.
    #[cfg(windows)]
    pub(super) fn track_hover_pointer(&mut self, main: isize, scale: f32, cx: &mut Context<Self>) {
        let request = (self.hover_preview.id, self.hover_preview.generation);
        self.hover_task = Some(cx.spawn(async move |view, cx| {
            use clipboard_platform::external_drag as native;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let Ok(pointer) = native::poll_drag_pointer() else {
                    continue;
                };
                let Ok((x, y)) = native::drag_pointer_in_client(main, pointer, scale) else {
                    break;
                };
                let valid = view
                    .update(cx, |this, cx| {
                        if (this.hover_preview.id, this.hover_preview.generation) != request {
                            return false;
                        }
                        if !this.hover_overlay_stack_allows_tracking(cx) {
                            this.close_hover_preview("overlay_stack_active", cx);
                            return false;
                        }
                        let preview_hwnd = this.hover_popup.as_ref().and_then(|handle| {
                            handle
                                .update(cx, |_, window, _| native_window::window_id(window))
                                .ok()
                                .flatten()
                        });
                        let over_main = native::pointer_targets_window(main, pointer);
                        let over_preview = preview_hwnd
                            .is_some_and(|hwnd| native::pointer_targets_window(hwnd, pointer));
                        if !over_main && !over_preview {
                            this.close_hover_preview("pointer_left_main", cx);
                            return false;
                        }
                        let source = this.drag_viewport.get().is_some_and(|viewport| {
                            if !viewport.contains(&point(px(x), px(y))) {
                                return false;
                            }
                            this.history
                                .items
                                .iter()
                                .position(|item| Some(item.id) == request.0)
                                .and_then(|index| {
                                    this.row_offsets.get(index).map(|offset| {
                                        let top = viewport.top() + *offset + this.scroll.offset().y;
                                        px(y) >= top
                                            && px(y)
                                                < top
                                                    + px(history_row_height(
                                                        &this.history.items[index],
                                                        this.display,
                                                    ))
                                    })
                                })
                                .unwrap_or(false)
                        });
                        if !source {
                            this.close_hover_preview("source_pointer_left", cx);
                            return false;
                        }
                        if let Some(id) = request.0 {
                            this.update_hover_anchor(id, px(y), cx);
                        }
                        true
                    })
                    .unwrap_or(false);
                if !valid {
                    break;
                }
            }
        }));
    }

    pub(super) fn zoom_hover_from_card(
        &mut self,
        id: i64,
        delta: Pixels,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.hover_preview.id != Some(id) || !self.hover_source_active || self.batch_mode {
            return false;
        }
        let Some(handle) = self
            .hover_popup
            .and_then(|handle| handle.downcast::<HoverPreviewWindowView>())
        else {
            return false;
        };
        let generation = self.hover_preview.generation;
        tracing::debug!(
            id,
            generation,
            delta = f32::from(delta),
            "Hover zoom routed from source card"
        );
        handle
            .update(cx, |preview, window, cx| {
                preview.id == id
                    && preview.generation == generation
                    && preview.zoom_by_wheel(delta, window, cx)
            })
            .unwrap_or(false)
    }

    pub(super) fn hover_source_allowed(&self, window: &Window) -> bool {
        if self.hover_overlay_stack_blocks_source() {
            return false;
        }
        if !window.is_window_active() {
            return false;
        }
        #[cfg(windows)]
        {
            use clipboard_platform::external_drag as native;
            let Some(main) = native_window::window_id(window) else {
                return false;
            };
            let Ok(pointer) = native::poll_drag_pointer() else {
                return true;
            };
            native::pointer_targets_window(main, pointer)
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    fn hover_overlay_stack_blocks_source(&self) -> bool {
        self.settings_window.is_some()
            || self.settings_window_opening
            || self.onboarding_visible()
            || self.confirmation.is_some()
    }

    /// 历史右键菜单当前是否活着；`update` 失败说明窗口已被移除。
    /// set_hover_source 的进入守卫与悬停跟踪共用同一判定，避免两处反向编码漂移。
    fn history_menu_open(&self, cx: &mut App) -> bool {
        self.history_menu_window
            .and_then(|menu| menu.downcast::<menu_window::HistoryMenuWindow>())
            .is_some_and(|menu| menu.update(cx, |_, _, _| ()).is_ok())
    }

    fn hover_overlay_stack_allows_tracking(&self, cx: &mut App) -> bool {
        !self.hover_overlay_stack_blocks_source() && !self.history_menu_open(cx)
    }

    pub(super) fn set_hover_source(
        &mut self,
        id: i64,
        entered: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        tracing::debug!(
            id, entered, current_id = ?self.hover_preview.id,
            generation = self.hover_preview.generation, "Hover source transition"
        );
        if crate::capabilities::positioning_unavailable_reason(self.language).is_some() {
            return;
        }
        if entered {
            // 先跑零开销的纯状态守卫；hover_source_allowed 里的指针/窗口命中是 Win32
            // 调用，on_mouse_move 会在这些守卫已拦截的情况下每像素触发一次。
            if self.batch_mode
                || cx.has_active_drag()
                || self.external_drag.is_some()
                || self.history.loading
                || self.editor.id.is_some()
                || self.pending_row_click.is_some()
                || self.history_menu_open(cx)
            {
                return;
            }
            if !self.hover_source_allowed(window) {
                if self.hover_preview.id.is_some() {
                    self.close_hover_preview("hover_source_blocked", cx);
                }
                return;
            }
            let Some(item) = self.history.items.iter().find(|item| item.id == id) else {
                return;
            };
            // 「文件」记录若恰好是一张图片文件，按「图片」开关判定。复制截图（PixPin 等
            // 工具）时剪贴板里放的是图片**文件**而不是 image 记录，若按「文件」开关判定，
            // 用户明明开了「图片」却看不到任何预览。
            let content_kind = if item.content_type == "files"
                && single_image_file_paths(item.file_paths.as_deref())
            {
                "image"
            } else {
                item.content_type.as_str()
            };
            let allowed = self.hover_preference.allows(content_kind);
            if !allowed {
                if self.hover_preview.id.is_some() {
                    self.close_hover_preview("content_kind_disabled", cx);
                }
                return;
            }
            if self.hover_preview.id != Some(id) {
                self.close_hover_preview("source_changed", cx);
                let generation = self.hover_preview.open(id);
                let delay = self.hover_preference.delay_ms;
                tracing::debug!(
                    id,
                    generation,
                    delay_ms = delay,
                    "Scheduling hover preview request"
                );
                self.hover_task = Some(cx.spawn(async move |view, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(u64::from(delay)))
                        .await;
                    let _ = view.update(cx, |this, cx| {
                        if this.hover_preview.id == Some(id)
                            && this.hover_preview.generation == generation
                            && this.hover_source_active
                            && !this.hover_overlay_stack_blocks_source()
                        {
                            tracing::debug!(id, generation, "Sending hover preview request");
                            this.send(Command::HoverPreview { id, generation }, cx);
                        }
                    });
                }));
            }
            self.hover_source_active = true;
        } else if self.hover_preview.id == Some(id) {
            self.close_hover_preview("source_left", cx);
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
}
