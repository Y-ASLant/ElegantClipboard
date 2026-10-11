use super::*;

impl ClipboardView {
    pub(super) fn save_display(&mut self, display: DisplayPreference, cx: &mut Context<Self>) {
        if !self.display_pending
            && display != self.display
            && self.send(Command::SetDisplay(display), cx)
        {
            self.display_pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_audio(&mut self, audio: AudioPreference, cx: &mut Context<Self>) {
        if !self.audio_pending && audio != self.audio && self.send(Command::SetAudio(audio), cx) {
            self.audio_pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_monitor_types(
        &mut self,
        monitor_types: MonitorTypesPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.monitor_types_pending
            && monitor_types.valid()
            && monitor_types != self.monitor_types
            && self.send(Command::SetMonitorTypes(monitor_types), cx)
        {
            self.monitor_types_pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_app_filter(
        &mut self,
        preference: AppFilterPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.app_filter_pending
            && preference.valid()
            && preference != self.app_filter
            && self.send(Command::SetAppFilter(Box::new(preference)), cx)
        {
            self.app_filter_pending = true;
            cx.notify();
        }
    }

    pub(super) fn load_running_apps(&mut self, cx: &mut Context<Self>) {
        if !self.running_apps_pending && self.send(Command::ListRunningApps, cx) {
            self.running_apps_pending = true;
            cx.notify();
        }
    }
    /// 统一处理“设置已保存”确认：清除 pending 标记、回写新值并按结果
    /// 设置状态栏消息。仅适用于形状完全一致的单值偏好确认。
    pub(super) fn acknowledge_save<T: Copy>(
        &mut self,
        result: Result<T, String>,
        set_pending: impl FnOnce(&mut Self, bool),
        set_value: impl FnOnce(&mut Self, T),
        saved_message: (&'static str, &'static str),
        failure_message: (&'static str, &'static str),
    ) {
        set_pending(self, false);
        match result {
            Ok(value) => {
                set_value(self, value);
                self.message = tr(self.language, saved_message.0, saved_message.1).into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(self.language, failure_message.0, failure_message.1)
                );
                self.is_error = true;
            }
        }
    }
    pub(super) fn apply_theme_saved(
        &mut self,
        result: Result<ThemePreference, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.theme_pending = false;
        match result {
            Ok(theme) => {
                self.close_hover_preview(cx);
                self.theme = theme;
                apply_theme(theme, window, cx);
                self.message = tr(
                    self.language,
                    "外观设置已保存",
                    "Appearance preference saved",
                )
                .into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(self.language, "外观保存失败", "Failed to save appearance")
                );
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_language_saved(
        &mut self,
        result: Result<LanguagePreference, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.language_pending = false;
        match result {
            Ok(language) => {
                self.close_hover_preview(cx);
                self.language = language;
                self.search.update(cx, |input, cx| {
                    input.set_placeholder(
                        tr(language, "搜索剪贴板历史…", "Search clipboard history…"),
                        window,
                        cx,
                    );
                });
                self.group_name_input.update(cx, |input, cx| {
                    input.set_placeholder(tr(language, "分组名称", "Group name"), window, cx);
                });
                self.refresh_group_select(window, cx);
                let menu_result = if let Some(tray) = &self._tray {
                    tray::update_menu(tray, language, self.paused, self.monitoring).map(|_| None)
                } else {
                    tray::create(
                        self.tray_sender.clone(),
                        language,
                        self.paused,
                        self.monitoring,
                    )
                    .map(Some)
                };
                match menu_result {
                    Ok(tray) => {
                        if let Some(tray) = tray {
                            self._tray = Some(tray);
                            self.tray_enabled.set(true);
                        }
                        self.message =
                            tr(language, "语言设置已保存", "Language preference saved").into();
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.message = format!(
                            "{}: {error}",
                            tr(
                                language,
                                "语言已保存，但托盘菜单更新失败",
                                "Language saved, but the tray menu could not be updated"
                            )
                        );
                        self.is_error = true;
                    }
                }
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(self.language, "语言保存失败", "Failed to save language")
                );
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn apply_window_size_saved(
        &mut self,
        result: Result<WindowSizePreference, String>,
    ) -> bool {
        match result {
            Ok(size) => self.last_window_size = Some(size),
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存窗口大小失败",
                        "Failed to save window size"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_persist_window_size_saved(
        &mut self,
        result: Result<bool, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.persist_window_size_pending = false;
        match result {
            Ok(enabled) => {
                self.persist_window_size = enabled;
                self.last_window_size = None;
                if enabled {
                    self.window_bounds_changed(window, cx);
                }
                self.message = tr(
                    self.language,
                    "窗口大小设置已保存",
                    "Window size setting saved",
                )
                .into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存窗口大小设置失败",
                        "Failed to save window size setting"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn apply_display_saved(
        &mut self,
        result: Result<DisplayPreference, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.display_pending = false;
        match result {
            Ok(display) => {
                let (top_index, partial) =
                    history_top_row(&self.row_offsets, self.scroll.offset().y);
                self.display = display;
                self.rebuild_row_layout();
                self.scroll.set_offset(point(
                    px(0.),
                    -(self.row_offsets[top_index.min(self.history.items.len())] + partial),
                ));
                if self
                    .history
                    .should_reset_category_filter(display.show_category_filter)
                {
                    self.select_category(Some(ContentCategory::All), window, cx);
                }
                self.message = tr(self.language, "显示设置已保存", "Display settings saved").into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存显示设置失败",
                        "Failed to save display settings"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_audio_saved(&mut self, result: Result<AudioPreference, String>) -> bool {
        self.acknowledge_save(
            result,
            |this, pending| this.audio_pending = pending,
            |this, audio| this.audio = audio,
            ("音效设置已保存", "Audio settings saved"),
            ("保存音效设置失败", "Failed to save audio settings"),
        );
        true
    }

    pub(super) fn apply_monitor_types_saved(
        &mut self,
        result: Result<MonitorTypesPreference, String>,
    ) -> bool {
        self.acknowledge_save(
            result,
            |this, pending| this.monitor_types_pending = pending,
            |this, preference| this.monitor_types = preference,
            ("监听类型已保存", "Capture types saved"),
            ("保存监听类型失败", "Failed to save capture types"),
        );
        true
    }

    pub(super) fn apply_app_filter_saved(
        &mut self,
        result: Result<Box<AppFilterPreference>, String>,
    ) -> bool {
        self.app_filter_pending = false;
        match result {
            Ok(preference) => {
                self.app_filter = *preference;
                self.message = tr(self.language, "应用过滤设置已保存", "App filter saved").into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存应用过滤失败",
                        "Failed to save app filter"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_onboarding_completed(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.onboarding_pending = false;
        match result {
            Ok(()) => {
                self.onboarding_completed = true;
                self.message = tr(
                    self.language,
                    "欢迎使用 ElegantClipboard",
                    "Welcome to ElegantClipboard",
                )
                .into();
                self.is_error = false;
                if self.search_auto_focus.value {
                    self.search.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.list_focus, cx);
                }
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存引导状态失败",
                        "Failed to save onboarding state"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_window_position_saved(
        &mut self,
        result: Result<WindowPositionPreference, String>,
    ) -> bool {
        self.acknowledge_save(
            result,
            |this, pending| this.window_position_pending = pending,
            |this, position| this.window_position = position,
            ("窗口唤出位置已保存", "Window position mode saved"),
            ("保存窗口位置失败", "Failed to save window position"),
        );
        true
    }

    pub(super) fn apply_autostart_saved(&mut self, result: Result<bool, String>) -> bool {
        self.autostart_pending = false;
        match result {
            Ok(enabled) => {
                self.autostart = enabled;
                self.message = if enabled {
                    tr(self.language, "已开启开机启动", "Startup enabled")
                } else {
                    tr(self.language, "已关闭开机启动", "Startup disabled")
                }
                .into();
                self.is_error = false;
            }
            Err(error) => {
                self.message = format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "开机启动设置失败",
                        "Failed to update startup"
                    )
                );
                self.is_error = true;
            }
        }
        true
    }
}
