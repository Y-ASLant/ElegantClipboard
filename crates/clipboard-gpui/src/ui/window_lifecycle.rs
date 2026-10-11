use super::*;
// Settings also observes the view, so its implicit GPUI window association can
// point at the settings window. Flush effects even when the main window is hidden.
pub(super) fn update_clipboard_window<R>(
    view: &WeakEntity<ClipboardView>,
    handle: AnyWindowHandle,
    cx: &AsyncApp,
    update: impl FnOnce(&mut ClipboardView, &mut Window, &mut Context<ClipboardView>) -> R,
) -> anyhow::Result<R> {
    cx.update(|cx| {
        handle.update(cx, |_, window, cx| {
            view.update(cx, |view, cx| update(view, window, cx))
        })?
    })
}

impl ClipboardView {
    pub(super) fn refresh_tray_menu(&self) -> anyhow::Result<()> {
        if let Some(tray) = &self._tray {
            tray::update_menu(tray, self.language, self.paused, self.monitoring)?;
        }
        Ok(())
    }
    pub(super) fn window_bounds_changed(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.persist_window_size || self.persist_window_size_pending {
            return;
        }
        let WindowBounds::Windowed(bounds) = window.window_bounds() else {
            return;
        };
        let Some(size) = WindowSizePreference::new(
            f32::from(bounds.size.width).round() as u32,
            f32::from(bounds.size.height).round() as u32,
        ) else {
            return;
        };
        if self.last_window_size == Some(size) {
            return;
        }
        self.window_size_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = view.update(cx, |this, cx| {
                if this.persist_window_size && !this.persist_window_size_pending {
                    this.send(Command::SetWindowSize(size), cx);
                }
            });
        }));
    }
    pub(super) fn handle_outside_click(
        &mut self,
        position: windows::Win32::Foundation::POINT,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !tray::is_window_shown(window)
            || self._tray.is_none()
            || self.settings_window.is_some()
            || self.settings_window_opening
            || self.paste_pending.is_some()
            || self.batch_paste_pending.is_some()
            || self.batch_pending
            || self.confirmation.is_some()
            || self
                ._tray
                .as_ref()
                .is_some_and(|tray| tray::is_tray_click(tray, position))
        {
            return;
        }
        self.prepare_to_hide(window, cx);
        tray::set_window_visible(window, false);
        cx.notify();
    }

    pub(super) fn show_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_minimized = tray::is_window_minimized(window);
        if was_minimized {
            tray::set_window_visible(window, true);
        }
        if let Err(error) = position::position_window(window, self.window_position) {
            self.message = format!(
                "{}: {error}",
                tr(self.language, "无法定位窗口", "Failed to position window")
            );
            self.is_error = true;
        }
        if !was_minimized {
            tray::set_window_visible(window, true);
        }
        if self.search_auto_clear.value && !self.search.read(cx).value().is_empty() {
            self.search_task = None;
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.history.begin_search();
            self.scroll_to_top();
            self.query(cx);
        }
        if self.preview_editing || self.preview_save_pending {
            return;
        }
        if self.search_auto_focus.value {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        } else if self.preview.id.is_none() {
            window.focus(&self.list_focus, cx);
        }
    }

    pub(super) fn hide_visible_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.paste_pending.is_some()
            || self.batch_paste_pending.is_some()
            || self.batch_pending
            || self.group_delete_pending
            || self.clear_pending
        {
            return;
        }
        if let Some(kind) = self.confirmation {
            self.cancel_confirmation(kind, cx);
            window.close_dialog(cx);
        }
        self.prepare_to_hide(window, cx);
        tray::set_window_visible(window, false);
        cx.notify();
    }

    pub(super) fn prepare_to_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        self.paste_target = None;
        self.reset_selection();
        if self.auto_reset_state.value {
            self.search_task = None;
            self.search
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.history.set_group(None);
            self.refresh_group_select(window, cx);
            self.scroll_to_top();
            if !self.preview_editing && !self.preview_save_pending {
                self.preview.close();
                self.preview_source_hash = None;
                self.preview_edit_requested = false;
            }
            self.query(cx);
        }
    }
    /// 当前打开的覆盖层，按 Esc 关闭优先级排列（同时至多一层）。
    pub(super) fn active_overlay(&self) -> Option<Overlay> {
        if self.group_delete_id.is_some() {
            Some(Overlay::GroupDelete)
        } else if self.clear_confirm_open {
            Some(Overlay::ClearConfirm)
        } else if self.batch_confirm_open {
            Some(Overlay::BatchConfirm)
        } else if self.group_move_id.is_some() {
            Some(Overlay::GroupMove)
        } else if self.group_editor_open {
            Some(Overlay::GroupEditor)
        } else if self.batch_mode {
            Some(Overlay::BatchMode)
        } else {
            None
        }
    }

    pub(super) fn dismiss_or_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.onboarding_visible() {
            self.complete_onboarding(cx);
            return;
        }
        self.cancel_pending_row_click();
        self.close_hover_preview(cx);
        if cx.has_active_drag() {
            cx.stop_active_drag(window);
            self.drop_target = None;
            self.history_drag_direction = 0;
            cx.notify();
            return;
        }
        match self.active_overlay() {
            Some(Overlay::GroupDelete) => {
                if !self.group_delete_pending {
                    self.group_delete_id = None;
                    self.sync_confirmation_dialog(window, cx);
                    cx.notify();
                }
            }
            Some(Overlay::ClearConfirm) => {
                if !self.clear_pending {
                    self.clear_confirm_open = false;
                    self.sync_confirmation_dialog(window, cx);
                    cx.notify();
                }
            }
            Some(Overlay::BatchConfirm) => {
                if !self.batch_pending {
                    self.batch_confirm_open = false;
                    self.sync_confirmation_dialog(window, cx);
                    cx.notify();
                }
            }
            Some(Overlay::GroupMove) => {
                if !self.group_move_pending {
                    self.group_move_id = None;
                    cx.notify();
                }
            }
            Some(Overlay::GroupEditor) => {
                if self.cancel_group_edit(window, cx) {
                    window.close_dialog(cx);
                }
            }
            Some(Overlay::BatchMode) => {
                if !self.batch_pending {
                    self.reset_selection();
                    cx.notify();
                }
            }
            None => {
                if self.paste_pending.is_some() || self.batch_paste_pending.is_some() {
                    return;
                }
                self.paste_target = None;
                if self._tray.is_some() {
                    self.prepare_to_hide(window, cx);
                    tray::set_window_visible(window, false);
                    cx.notify();
                } else {
                    self.exiting.set(true);
                    window.remove_window();
                }
            }
        }
    }
}
