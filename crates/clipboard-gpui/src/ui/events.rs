use super::*;

impl ClipboardView {
    pub(super) fn send(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        if matches!(
            &command,
            Command::Delete(_)
                | Command::DeleteBatch { .. }
                | Command::DeleteGroup { .. }
                | Command::ClearHistory { .. }
                | Command::ClearAllHistory
        ) {
            self.cancel_sequence(cx);
        }
        let replacement = self.paste_state.replace(&command);
        let replacing_clipboard = replacement.is_some();
        let resolving_quick = matches!(&command, Command::ResolveQuickPaste { .. });
        if let Some(interrupted) = replacement {
            self.apply_paste_interruption(interrupted);
        }
        if let Err(error) = self.service.send(command) {
            tracing::error!(%error, "Sending backend command failed");
            self.set_feedback_message(error.to_string());
            self.is_error = true;
            cx.notify();
            return false;
        }
        if replacing_clipboard
            && !resolving_quick
            && sound::enabled(self.audio.value, Sound::Copy, SoundTiming::Immediate)
        {
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
                self.paste_state.set_target(None);
                self.show_window(window, cx);
                true
            }
            Event::Groups(groups) => self.apply_groups(groups, window, cx),
            Event::GroupReordered { result, .. } => self.apply_group_reordered(result),
            Event::GroupCreated(result) => self.apply_group_created(result, window, cx),
            Event::GroupRenamed(result) => self.apply_group_renamed(result, window, cx),
            Event::GroupDeleted(result) => self.apply_group_deleted(result, window, cx),
            Event::ItemDeleted { id, outcome } => self.apply_item_deleted(id, outcome, cx),
            Event::HistoryCleared(result) => self.apply_history_cleared(result, window, cx),
            Event::AllHistoryCleared(result) => self.apply_all_history_cleared(result, window, cx),
            Event::BatchDeleted(result) => self.apply_batch_deleted(result, window, cx),
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
            } => self.apply_editor_content(id, generation, result, window, cx),
            Event::HoverPreview {
                id,
                generation,
                result,
            } => self.apply_hover_preview_result(id, generation, result, window, cx),
            Event::HoverPreviewSaved(result) => self.apply_hover_preview_saved(result, cx),
            Event::RetentionSaved(result) => self.apply_retention_saved(result, cx),
            Event::TextEdited {
                id,
                generation,
                result,
            } => self.apply_text_edited(id, generation, result, window, cx),
            Event::SavedAs { id, result } => self.apply_saved_as(id, result),
            Event::ExternalDragPrepared { id, token, result } => {
                self.apply_external_drag_prepared(id, token, result, cx);
                true
            }
            Event::Reordered {
                from,
                generation,
                result,
            } => self.apply_reordered(from, generation, result, cx),
            Event::Status(message) => {
                self.set_feedback_message(localize_service_message(self.language, message));
                self.is_error = false;
                true
            }
            Event::Copied {
                id,
                purpose,
                clipboard_sequence,
                message,
            } => self.apply_copied(id, purpose, clipboard_sequence, message, window, cx),
            Event::Merged {
                purpose,
                clipboard_sequence,
                item_count,
            } => self.apply_merged(purpose, clipboard_sequence, item_count, window, cx),
            Event::ThemeSaved(result) => self.apply_theme_saved(result, window, cx),
            Event::CornerRadiusSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.corner_radius,
                    ("圆角设置已保存", "Corner radius saved"),
                    ("圆角保存失败", "Failed to save corner radius"),
                );
                crate::design_system::set_corner_radius(self.corner_radius.value, cx);
                true
            }
            Event::WindowEffectSaved(result) => self.apply_window_effect_saved(result, window, cx),
            Event::LanguageSaved(result) => self.apply_language_saved(result, window, cx),
            Event::HotkeySaved(result) => self.apply_hotkey_saved(result),
            Event::WindowSizeSaved(result) => self.apply_window_size_saved(result),
            Event::PersistWindowSizeSaved(result) => {
                self.apply_persist_window_size_saved(result, window, cx)
            }
            Event::AutoResetStateSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.auto_reset_state,
                    ("隐藏时重置设置已保存", "Reset-on-hide setting saved"),
                    ("保存隐藏设置失败", "Failed to save hide setting"),
                );
                true
            }
            Event::SearchAutoFocusSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.search_auto_focus,
                    ("搜索焦点设置已保存", "Search focus setting saved"),
                    ("保存搜索焦点失败", "Failed to save search focus"),
                );
                true
            }
            Event::SearchAutoClearSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.search_auto_clear,
                    ("搜索清空设置已保存", "Search clearing setting saved"),
                    ("保存搜索清空失败", "Failed to save search clearing"),
                );
                true
            }
            Event::SkipClearConfirmSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.skip_clear_confirm,
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
                    |this| &mut this.paste_close_window,
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
                    |this| &mut this.paste_key,
                    ("粘贴按键设置已保存", "Paste key setting saved"),
                    ("保存粘贴按键失败", "Failed to save paste key"),
                );
                true
            }
            Event::PasteMoveToTopSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.paste_move_to_top,
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
            Event::DedupStrategySaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.dedup_strategy,
                    ("去重策略已保存", "Dedup strategy saved"),
                    ("保存去重策略失败", "Failed to save dedup strategy"),
                );
                true
            }
            Event::RestoreClipboardSaved(result) => {
                self.acknowledge_save(
                    result,
                    |this| &mut this.restore_clipboard,
                    ("恢复剪贴板设置已保存", "Clipboard restore setting saved"),
                    (
                        "保存恢复剪贴板设置失败",
                        "Failed to save clipboard restore setting",
                    ),
                );
                true
            }
            Event::SequenceHotkeySaved(result) => self.apply_sequence_hotkey_saved(result),
            Event::NoteSaved(result) => self.apply_note_saved(result, window, cx),
            Event::ClipboardRestored(result) => match result {
                // 没有快照（偏好关闭或读取失败）不打扰用户；失败要提示。
                Ok(_) => false,
                Err(error) => {
                    self.set_feedback_message(format!(
                        "{}: {error}",
                        tr(
                            self.language,
                            "恢复原剪贴板失败",
                            "Failed to restore clipboard"
                        )
                    ));
                    self.is_error = true;
                    true
                }
            },
            Event::QuickPasteEnabledSaved(result) => self.apply_quick_paste_enabled_saved(result),
            Event::PasteShortcutsSaved(result) => self.apply_paste_shortcuts_saved(result),
            Event::QuickPasteResolved { request_id, result } => {
                self.apply_quick_paste_resolved(request_id, result, window, cx)
            }
            Event::DisplaySaved(result) => self.apply_display_saved(result, window, cx),
            Event::AudioSaved(result) => self.apply_audio_saved(result),
            Event::TransientFilterSaved(result) => self.apply_transient_filter_saved(result),
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
                self.set_feedback_message(
                    tr(
                        self.language,
                        "数据库已整理，数据占用已更新",
                        "Database optimized and storage usage updated",
                    )
                    .into(),
                );
                self.is_error = false;
                true
            }
            Event::Paused(paused) => {
                self.paused = paused;
                self.pause_pending = false;
                self.is_error = false;
                self.set_feedback_message(
                    if paused {
                        tr(
                            self.language,
                            "已暂停记录，已有历史仍可使用",
                            "Recording paused; existing history is still available",
                        )
                    } else {
                        tr(self.language, "已恢复记录", "Recording resumed")
                    }
                    .into(),
                );
                let _ = self.refresh_tray_menu();
                true
            }
            Event::BackupExported(result) => self.apply_backup_exported(result),
            Event::CommandFailed { kind, message } => {
                tracing::error!(?kind, "Backend command failed; details shown in UI");
                self.apply_command_failed(kind, message, window, cx)
            }
            Event::Error(message) => {
                tracing::error!("Backend error received; details shown in UI");
                self.set_feedback_message(message);
                self.is_error = true;
                self.history.loading = false;
                true
            }
            Event::BackgroundError(message) => {
                tracing::error!("Backend background error received; details shown in UI");
                self.set_feedback_message(message);
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
            && (self.editor.id != Some(id) || self.editor.generation != generation)
        {
            return false;
        }
        match kind {
            FailureKind::Query(_) => {}
            FailureKind::Paste(request_id) => {
                let Some(interrupted) = self.paste_state.fail(request_id) else {
                    return false;
                };
                self.apply_paste_interruption(interrupted);
            }
            FailureKind::QuickPaste(request_id) => {
                if self.paste_state.resolve_quick(request_id).is_none() {
                    return false;
                }
            }
            FailureKind::Merge => {
                self.batch_pending = false;
                self.paste_state.finish_merge();
            }
            FailureKind::GroupSave => {
                self.group_save_pending = false;
                self.group_edit_error = Some(message.clone());
                window.refresh();
            }
            FailureKind::GroupDelete => {
                self.group_delete_pending = false;
                if matches!(self.confirmation, Some(HistoryConfirmation::DeleteGroup(_))) {
                    self.close_history_confirmation(window, cx);
                }
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
                if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
                    self.close_history_confirmation(window, cx);
                }
            }
            FailureKind::EditText { .. } => {
                self.editor_save_pending = false;
                self.editor_error = Some(message.clone());
                window.refresh();
            }
            FailureKind::SaveAs(id) if self.save_as_pending == Some(id) => {
                self.save_as_pending = None;
            }
            FailureKind::DataSize => self.data_size_pending = false,
            FailureKind::DatabaseMaintenance => {
                self.database_maintenance_pending = false;
            }
            FailureKind::Pause => self.pause_pending = false,
            FailureKind::Other if self.note_save_pending => {
                self.note_save_pending = false;
                self.editor_error = Some(message.clone());
                window.refresh();
            }
            FailureKind::Other | FailureKind::SaveAs(_) => {}
        }
        self.set_feedback_message(message);
        self.is_error = true;
        true
    }
}
