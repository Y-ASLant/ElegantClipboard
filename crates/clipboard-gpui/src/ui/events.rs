use super::*;

impl ClipboardView {
    pub(super) fn send(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        let copy_requested = matches!(
            &command,
            Command::Copy(_)
                | Command::CopyPlainText(_)
                | Command::CopyPlainTextForPaste(_)
                | Command::CopyForPaste(_)
                | Command::CopyPath(_)
                | Command::CopyPathForPaste(_)
                | Command::MergeCopy(_)
                | Command::MergeForPaste(_)
        );
        if let Err(error) = self.service.send(command) {
            self.message = error.to_string();
            self.is_error = true;
            cx.notify();
            return false;
        }
        if copy_requested && sound::enabled(self.audio, Sound::Copy, SoundTiming::Immediate) {
            sound::play(Sound::Copy);
        }
        true
    }
    pub(super) fn apply_event(
        &mut self,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let handled: bool = match event {
            Event::ShowWindow => {
                self.paste_target = None;
                self.show_window(window, cx);
                true
            }
            Event::Groups(groups) => self.apply_groups(groups, window, cx),
            Event::GroupReordered { result, .. } => self.apply_group_reordered(result),
            Event::GroupCreated(result) => self.apply_group_created(result, window, cx),
            Event::GroupRenamed(result) => self.apply_group_renamed(result, window, cx),
            Event::GroupDeleted(result) => self.apply_group_deleted(result, window, cx),
            Event::HistoryCleared(result) => self.apply_history_cleared(result),
            Event::AllHistoryCleared(result) => self.apply_all_history_cleared(result, window, cx),
            Event::BatchDeleted(result) => self.apply_batch_deleted(result),
            Event::ItemMoved { result, .. } => self.apply_item_moved(result),
            Event::Snapshot {
                items,
                total,
                generation,
            } => self.apply_snapshot(items, total, generation, cx),
            Event::Preview {
                id,
                generation,
                result,
            } => self.apply_preview_result(id, generation, result, window, cx),
            Event::HoverPreview {
                id,
                generation,
                result,
            } => self.apply_hover_preview_result(id, generation, result, window, cx),
            Event::HoverPreviewSaved(result) => self.apply_hover_preview_saved(result, cx),
            Event::TextEdited {
                id,
                generation,
                result,
            } => self.apply_text_edited(id, generation, result, window, cx),
            Event::SavedAs { id, result } => self.apply_saved_as(id, result),
            Event::Reordered {
                from,
                generation,
                result,
            } => self.apply_reordered(from, generation, result, cx),
            Event::Status(message) => {
                self.message = localize_service_message(self.language, message);
                self.is_error = false;
                true
            }
            Event::Copied {
                id,
                for_paste,
                clipboard_sequence,
                message,
            } => self.apply_copied(id, for_paste, clipboard_sequence, message, window, cx),
            Event::Merged {
                for_paste,
                clipboard_sequence,
                item_count,
            } => self.apply_merged(for_paste, clipboard_sequence, item_count, window, cx),
            Event::ThemeSaved(result) => self.apply_theme_saved(result, window, cx),
            Event::LanguageSaved(result) => self.apply_language_saved(result, window, cx),
            Event::HotkeySaved(result) => self.apply_hotkey_saved(result),
            Event::WindowSizeSaved(result) => self.apply_window_size_saved(result),
            Event::PersistWindowSizeSaved(result) => {
                self.apply_persist_window_size_saved(result, window, cx)
            }
            Event::AutoResetStateSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.auto_reset_state.pending = pending,
                    |this, value| this.auto_reset_state.value = value,
                    ("隐藏时重置设置已保存", "Reset-on-hide setting saved"),
                    ("保存隐藏设置失败", "Failed to save hide setting"),
                );
                true
            }
            Event::SearchAutoFocusSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.search_auto_focus.pending = pending,
                    |this, value| this.search_auto_focus.value = value,
                    ("搜索焦点设置已保存", "Search focus setting saved"),
                    ("保存搜索焦点失败", "Failed to save search focus"),
                );
                true
            }
            Event::SearchAutoClearSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.search_auto_clear.pending = pending,
                    |this, value| this.search_auto_clear.value = value,
                    ("搜索清空设置已保存", "Search clearing setting saved"),
                    ("保存搜索清空失败", "Failed to save search clearing"),
                );
                true
            }
            Event::SkipClearConfirmSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.skip_clear_confirm.pending = pending,
                    |this, value| this.skip_clear_confirm.value = value,
                    ("清理确认设置已保存", "Clear confirmation setting saved"),
                    (
                        "保存清理确认设置失败",
                        "Failed to save clear confirmation setting",
                    ),
                );
                true
            }
            Event::PasteCloseWindowSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.paste_close_window.pending = pending,
                    |this, value| this.paste_close_window.value = value,
                    (
                        "粘贴后关闭窗口设置已保存",
                        "Close-after-paste setting saved",
                    ),
                    (
                        "保存粘贴后窗口行为失败",
                        "Failed to save paste window behavior",
                    ),
                );
                true
            }
            Event::PasteKeySaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.paste_key_pending = pending,
                    |this, value| this.paste_key = value,
                    ("粘贴按键设置已保存", "Paste key setting saved"),
                    ("保存粘贴按键失败", "Failed to save paste key"),
                );
                true
            }
            Event::PasteMoveToTopSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this, pending| this.paste_move_to_top.pending = pending,
                    |this, value| this.paste_move_to_top.value = value,
                    (
                        "粘贴后移到首位设置已保存",
                        "Move-to-top-after-paste setting saved",
                    ),
                    (
                        "保存粘贴后排序设置失败",
                        "Failed to save paste sorting setting",
                    ),
                );
                true
            }
            Event::QuickPasteEnabledSaved(result) => self.apply_quick_paste_enabled_saved(result),
            Event::PasteShortcutsSaved(result) => self.apply_paste_shortcuts_saved(result),
            Event::QuickPasteResolved {
                slot,
                favorite,
                result,
            } => self.apply_quick_paste_resolved(slot, favorite, result, window, cx),
            Event::DisplaySaved(result) => self.apply_display_saved(result, window, cx),
            Event::AudioSaved(result) => self.apply_audio_saved(result),
            Event::MonitorTypesSaved(result) => self.apply_monitor_types_saved(result),
            Event::AppFilterSaved(result) => self.apply_app_filter_saved(result),
            Event::RunningApps(apps) => {
                self.running_apps_pending = false;
                self.running_apps = apps;
                true
            }
            Event::OnboardingCompleted(result) => {
                self.apply_onboarding_completed(result, window, cx)
            }
            Event::WindowPositionSaved(result) => self.apply_window_position_saved(result),
            Event::AutostartSaved(result) => self.apply_autostart_saved(result),
            Event::DataSize(result) => self.apply_data_size(result),
            Event::DailyCounts(result) => self.apply_daily_counts(result),
            Event::DatabaseOptimized(size) => {
                self.database_maintenance_pending = false;
                self.data_size = Some(size);
                self.message = tr(
                    self.language,
                    "数据库已整理，数据占用已更新",
                    "Database optimized and storage usage updated",
                )
                .into();
                self.is_error = false;
                true
            }
            Event::Paused(paused) => {
                self.paused = paused;
                self.pause_pending = false;
                self.is_error = false;
                self.message = if paused {
                    tr(
                        self.language,
                        "已暂停记录，已有历史仍可使用",
                        "Recording paused; existing history is still available",
                    )
                } else {
                    tr(self.language, "已恢复记录", "Recording resumed")
                }
                .into();
                let _ = self.refresh_tray_menu();
                true
            }
            Event::BackupExported(result) => self.apply_backup_exported(result),
            Event::CommandFailed { kind, message } => {
                self.apply_command_failed(kind, message, window, cx)
            }
            Event::Error(message) => {
                self.message = message;
                self.is_error = true;
                self.history.loading = false;
                true
            }
            Event::BackgroundError(message) => {
                self.message = message;
                self.is_error = true;
                true
            }
        };
        if !handled {
            return;
        }
        self.sync_confirmation_dialog(window, cx);
        if self.confirmation.is_some() {
            window.refresh();
        }
        cx.notify();
    }
    pub(super) fn apply_command_failed(
        &mut self,
        kind: FailureKind,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let FailureKind::Query(generation) = kind
            && !self.history.fail_query(generation)
        {
            return false;
        }
        if let FailureKind::EditText { id, generation } = kind
            && (self.preview.id != Some(id) || self.preview.generation != generation)
        {
            return false;
        }
        match kind {
            FailureKind::Query(_) => {}
            FailureKind::Paste(id)
                if self
                    .paste_pending
                    .is_some_and(|(pending_id, _)| pending_id == id) =>
            {
                self.paste_pending = None;
            }
            FailureKind::Merge => {
                self.batch_pending = false;
                self.batch_paste_pending = None;
            }
            FailureKind::GroupSave => {
                self.group_save_pending = false;
                self.group_edit_error = Some(message.clone());
                window.refresh();
            }
            FailureKind::GroupDelete => {
                self.group_delete_pending = false;
                self.group_delete_id = None;
            }
            FailureKind::GroupMove => self.group_move_pending = false,
            FailureKind::ClearHistory => {
                self.clear_pending = false;
                self.confirmation_error = Some(message.clone());
            }
            FailureKind::ClearAllHistory => {
                self.clear_all_pending = false;
                self.clear_all_error = Some(message.clone());
                cx.refresh_windows();
            }
            FailureKind::BatchDelete => {
                self.batch_pending = false;
                self.batch_confirm_open = false;
            }
            FailureKind::EditText { .. } => self.preview_save_pending = false,
            FailureKind::SaveAs(id) if self.save_as_pending == Some(id) => {
                self.save_as_pending = None;
            }
            FailureKind::DataSize => self.data_size_pending = false,
            FailureKind::DatabaseMaintenance => {
                self.database_maintenance_pending = false;
            }
            FailureKind::Pause => self.pause_pending = false,
            FailureKind::Other | FailureKind::Paste(_) | FailureKind::SaveAs(_) => {}
        }
        self.message = message;
        self.is_error = true;
        true
    }
}
