use super::window_effect_apply::apply_window_effect_to_open_windows;
use super::*;

impl<T> PendingPref<T> {
    fn acknowledge(&mut self, result: Result<T, String>) -> Result<(), String> {
        self.pending = false;
        self.value = result?;
        Ok(())
    }
}

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
        if !self.audio.pending
            && audio != self.audio.value
            && self.send(Command::SetAudio(audio), cx)
        {
            self.audio.pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_monitor_types(
        &mut self,
        monitor_types: MonitorTypesPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.monitor_types.pending
            && monitor_types.valid()
            && monitor_types != self.monitor_types.value
            && self.send(Command::SetMonitorTypes(monitor_types), cx)
        {
            self.monitor_types.pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_app_filter(
        &mut self,
        preference: AppFilterPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.app_filter.pending
            && preference.valid()
            && preference != self.app_filter.value
            && self.send(Command::SetAppFilter(Box::new(preference)), cx)
        {
            self.app_filter.pending = true;
            cx.notify();
        }
    }

    pub(super) fn save_window_effect(
        &mut self,
        preference: WindowEffectPreference,
        cx: &mut Context<Self>,
    ) {
        if !self.window_effect_pending
            && preference.valid()
            && preference != self.window_effect
            && self.send(Command::SetWindowEffect(preference), cx)
        {
            self.window_effect_pending = true;
            cx.notify();
        }
    }

    pub(super) fn load_running_apps(&mut self, cx: &mut Context<Self>) {
        if !self.running_apps_pending && self.send(Command::ListRunningApps, cx) {
            self.running_apps_pending = true;
            cx.notify();
        }
    }
    /// 普通偏好仅在保存成功后替换生效值；失败也必须释放 pending。
    pub(super) fn acknowledge_save<T>(
        &mut self,
        result: Result<T, String>,
        preference: impl FnOnce(&mut Self) -> &mut PendingPref<T>,
        saved_message: (&'static str, &'static str),
        failure_message: (&'static str, &'static str),
    ) {
        match preference(self).acknowledge(result) {
            Ok(()) => {
                self.set_feedback_message(
                    tr(self.language, saved_message.0, saved_message.1).into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, failure_message.0, failure_message.1)
                ));
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
                self.set_feedback_message(
                    tr(
                        self.language,
                        "外观设置已保存",
                        "Appearance preference saved",
                    )
                    .into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "外观保存失败", "Failed to save appearance")
                ));
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
                        self.set_feedback_message(
                            tr(language, "语言设置已保存", "Language preference saved").into(),
                        );
                        self.is_error = false;
                    }
                    Err(error) => {
                        self.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                language,
                                "语言已保存，但托盘菜单更新失败",
                                "Language saved, but the tray menu could not be updated"
                            )
                        ));
                        self.is_error = true;
                    }
                }
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "语言保存失败", "Failed to save language")
                ));
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
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存窗口大小失败",
                        "Failed to save window size"
                    )
                ));
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
                self.set_feedback_message(
                    tr(
                        self.language,
                        "窗口大小设置已保存",
                        "Window size setting saved",
                    )
                    .into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存窗口大小设置失败",
                        "Failed to save window size setting"
                    )
                ));
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
                self.display = display;
                self.rebuild_rows_keeping_scroll();
                if self
                    .history
                    .should_reset_category_filter(display.show_category_filter)
                {
                    self.select_category(Some(ContentCategory::All), window, cx);
                }
                self.set_feedback_message(
                    tr(self.language, "显示设置已保存", "Display settings saved").into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存显示设置失败",
                        "Failed to save display settings"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_audio_saved(&mut self, result: Result<AudioPreference, String>) -> bool {
        self.acknowledge_save(
            result,
            |this| &mut this.audio,
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
            |this| &mut this.monitor_types,
            ("监听类型已保存", "Capture types saved"),
            ("保存监听类型失败", "Failed to save capture types"),
        );
        true
    }

    pub(super) fn apply_app_filter_saved(
        &mut self,
        result: Result<Box<AppFilterPreference>, String>,
    ) -> bool {
        self.acknowledge_save(
            result.map(|preference| *preference),
            |this| &mut this.app_filter,
            ("应用过滤设置已保存", "App filter saved"),
            ("保存应用过滤失败", "Failed to save app filter"),
        );
        true
    }

    pub(super) fn apply_window_effect_saved(
        &mut self,
        result: Result<WindowEffectPreference, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.window_effect_pending = false;
        match result {
            Ok(preference) => {
                if preference != self.window_effect {
                    let capabilities =
                        clipboard_platform::window_effect::WindowEffectCapabilities::current();
                    let material_changed = capabilities.resolve(preference.effect)
                        != capabilities.resolve(self.window_effect.effect);
                    self.window_effect = preference;
                    acrylic::record(preference, cx);
                    if material_changed {
                        self.close_hover_preview(cx);
                        apply_window_effect_to_open_windows(preference.effect, window, cx);
                    }
                    cx.refresh_windows();
                }
                self.set_feedback_message(
                    tr(
                        self.language,
                        "窗口效果设置已保存",
                        "Window effect settings saved",
                    )
                    .into(),
                );
                self.is_error = false;
                cx.notify();
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "窗口效果保存失败",
                        "Failed to save window effect"
                    )
                ));
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
                self.set_feedback_message(
                    tr(
                        self.language,
                        "欢迎使用 ElegantClipboard",
                        "Welcome to ElegantClipboard",
                    )
                    .into(),
                );
                self.is_error = false;
                if self.search_auto_focus.value {
                    self.search.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.list_focus, cx);
                }
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存引导状态失败",
                        "Failed to save onboarding state"
                    )
                ));
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
            |this| &mut this.window_position,
            ("窗口唤出位置已保存", "Window position mode saved"),
            ("保存窗口位置失败", "Failed to save window position"),
        );
        true
    }

    pub(super) fn apply_autostart_saved(&mut self, result: Result<bool, String>) -> bool {
        let saved_message = if matches!(result, Ok(true)) {
            ("已开启开机启动", "Startup enabled")
        } else {
            ("已关闭开机启动", "Startup disabled")
        };
        self.acknowledge_save(
            result,
            |this| &mut this.autostart,
            saved_message,
            ("开机启动设置失败", "Failed to update startup"),
        );
        true
    }

    pub(super) fn apply_transient_filter_saved(&mut self, result: Result<bool, String>) -> bool {
        self.acknowledge_save(
            result,
            |this| &mut this.transient_filter,
            (
                "瞬时内容过滤设置已保存",
                "Transient filtering setting saved",
            ),
            (
                "保存瞬时内容过滤设置失败",
                "Failed to save transient filtering setting",
            ),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PendingPref;

    #[test]
    fn failed_preference_save_keeps_effective_value_and_allows_retry() {
        let mut preference = PendingPref {
            value: String::from("saved"),
            pending: true,
        };
        let error = String::from("storage unavailable");
        assert_eq!(preference.acknowledge(Err(error.clone())), Err(error));
        assert_eq!(preference.value, "saved");
        assert!(!preference.pending);

        preference.pending = true;
        assert_eq!(preference.acknowledge(Ok(String::from("retried"))), Ok(()));
        assert_eq!(preference.value, "retried");
        assert!(!preference.pending);
    }
}
