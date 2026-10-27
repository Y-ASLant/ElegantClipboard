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
    /// 托盘命令事件循环（从 view_init 拆分）。
    pub(super) fn spawn_tray_events(
        receiver: async_channel::Receiver<TrayCommand>,
        main_window: AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |view, cx| {
            while let Ok(command) = receiver.recv().await {
                if update_clipboard_window(&view, main_window, cx, |this, window, cx| match command
                {
                    TrayCommand::Toggle => {
                        if native_window::is_window_shown(window)
                            && !native_window::is_window_minimized(window)
                        {
                            this.hide_visible_window(window, cx);
                        } else {
                            this.paste_target = None;
                            this.show_window(window, cx);
                            cx.notify();
                        }
                    }
                    TrayCommand::Settings => this.open_settings_window(SettingsPage::General, cx),
                    TrayCommand::ClearHistory => {
                        this.paste_target = None;
                        this.show_window(window, cx);
                        this.open_clear_history(window, cx);
                    }
                    TrayCommand::TogglePause => {
                        if this.monitoring
                            && !this.pause_pending
                            && this.send(Command::Pause(!this.paused), cx)
                        {
                            this.pause_pending = true;
                            cx.notify();
                        }
                    }
                    TrayCommand::Quit => {
                        this.exiting.set(true);
                        cx.quit();
                    }
                })
                .is_err()
                {
                    break;
                }
            }
        })
    }

    /// 开机启动设置读取失败的启动提示（仅失败时产生一条）。
    pub(super) fn autostart_startup_error(
        language: LanguagePreference,
        autostart: &Result<bool, anyhow::Error>,
    ) -> Option<String> {
        autostart.as_ref().err().map(|error| {
            format!(
                "{}: {error}",
                tr(
                    language,
                    "无法读取开机启动设置",
                    "Unable to read startup settings"
                )
            )
        })
    }

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
        position: (i32, i32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !native_window::is_window_shown(window)
            || self.external_drag.is_some()
            || cx.has_active_drag()
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
        self.hide_window(window, cx);
    }

    pub(super) fn show_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.paste_delivery_task = None;
        // 窗口又要显示了：取消隐藏时的延迟释放，避免缩略图白解码一遍。
        self.image_release_task = None;
        let was_minimized = native_window::is_window_minimized(window);
        if was_minimized {
            native_window::set_window_visible(window, true);
        }
        if let Err(error) = position::position_window(window, self.window_position.value) {
            self.set_feedback_message(format!(
                "{}: {error}",
                tr(self.language, "无法定位窗口", "Failed to position window")
            ));
            self.is_error = true;
        }
        if !was_minimized {
            native_window::set_window_visible(window, true);
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
        if self.card_drag_active(cx)
            || self.paste_pending.is_some()
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
        self.hide_window(window, cx);
    }

    /// 窗口失去激活时：如果用户是切去**别的应用**，就把悬停预览收掉，并丢弃待粘贴目标。
    ///
    /// 关键是把两种「主窗口失活」区分开：
    /// - 用户点到别的应用 → 我们确实不再被使用，预览该消失、自动粘贴也不可能完成；
    /// - 用户点的是**我们自己的浮窗**（比如按预览里的缩放按钮）→ 前台窗口仍然属于本进程，
    ///   只是在操作预览，什么都不该关。
    ///
    /// 判据用「前台窗口属于哪个进程」，**不要**用「浮窗此刻是不是激活窗口」——
    /// 失活回调触发的那一刻浮窗往往还没被系统标记为激活，会把点浮窗误判成切走，
    /// 结果预览被一点就没（之前就是这么错的）。
    pub(super) fn handle_window_deactivated(&mut self, cx: &mut Context<Self>) {
        if crate::native::foreground_window_is_ours() {
            return;
        }
        self.close_hover_preview(cx);
        self.cancel_pending_row_click();
        let had_target = self.paste_target.take().is_some();
        let had_pending = self.paste_pending.take().is_some();
        let had_batch_pending = self.batch_paste_pending.take().is_some();
        // 离开面板即取消序列（决策 0004）：目标已随失活失效。
        if self.sequence_queue.take().is_some() {
            self.reset_selection();
        }
        if had_target || had_pending || had_batch_pending {
            self.set_feedback_message(
                tr(
                    self.language,
                    "已离开历史窗口；如需自动粘贴，请从目标应用重新唤出",
                    "The history window lost focus. Open it again from the target app to paste automatically.",
                )
                .into(),
            );
            self.is_error = false;
            cx.notify();
        }
    }

    pub(super) fn hide_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.card_drag_active(cx) {
            return;
        }
        self.prepare_to_hide(window, cx);
        native_window::set_window_visible(window, false);
        self.schedule_image_release(cx);
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
            self.history.reset_filters();
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
        if let Some(kind) = self.confirmation {
            Some(Overlay::Confirmation(kind))
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
        if self.card_drag_active(cx) {
            cx.stop_active_drag(window);
            self.finish_external_drag(cx);
            self.drop_target = None;
            self.history_drag_direction = 0;
            cx.notify();
            return;
        }
        match self.active_overlay() {
            Some(Overlay::Confirmation(kind)) => {
                if self.cancel_confirmation(kind, cx) {
                    window.close_dialog(cx);
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
                    self.hide_window(window, cx);
                } else {
                    self.exiting.set(true);
                    window.remove_window();
                }
            }
        }
    }
}
