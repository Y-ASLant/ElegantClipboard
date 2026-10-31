use super::super::{
    data::data_size_info,
    file_actions::{item_paths_for_action, open_data_directory, reveal_in_explorer, save_item_as},
};
use super::{Command, Event, Worker, copy_mode::CopyMode};
use crate::{autostart, source_app};
use anyhow::{Context, Result};
use clipboard_core::preferences::Preferences;
use std::path::Path;

impl Worker {
    pub(in crate::service) fn handle(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Query {
                search,
                favorite_only,
                category,
                group_id,
                limit,
                generation,
            } => {
                self.search = search;
                self.favorite_only = favorite_only;
                self.category = category;
                self.group_id = group_id;
                self.limit = limit;
                self.generation = generation;
            }
            Command::Capture(text) => {
                self.history.capture_with_media(&text, &self.images_dir)?;
            }
            Command::CaptureRich { html, rtf, text } => {
                self.history.capture_rich(
                    html.as_deref(),
                    rtf.as_deref(),
                    text.as_deref(),
                    &self.images_dir,
                )?;
            }
            Command::CaptureFiles(paths) => {
                self.capture_files(&paths)?;
            }
            Command::CaptureImage { png, width, height } => {
                self.history
                    .capture_image(&png, width, height, &self.images_dir)?;
            }
            Command::ObservedCapture {
                content,
                source,
                extra,
                formats,
                deferred,
                ..
            } => {
                // deferred 仅由 service.rs 路由瞬时窗口；到达 worker 的命令一律直通。
                let _ = deferred;
                self.handle_observed_capture(content, extra, formats, source)?;
                self.snapshot_pending = true;
                return self.flush_capture_snapshot();
            }
            Command::MergeCopy(ids) => return self.handle_merge(ids, None),
            Command::MergeForPaste { ids, request_id } => {
                return self.handle_merge(ids, Some(request_id));
            }
            Command::RevealInExplorer(id) => {
                let item = self.history.item(id)?;
                let paths = item_paths_for_action(&item)?;
                reveal_in_explorer(Path::new(&paths[0]))?;
                self.send_event(Event::Status(if paths.len() == 1 {
                    "已在资源管理器中定位".into()
                } else {
                    format!("已在资源管理器中定位第一项，共 {} 项", paths.len())
                }))?;
                return Ok(());
            }
            Command::SaveAs { id, destination } => {
                let result = self
                    .history
                    .item(id)
                    .and_then(|item| save_item_as(&item, &destination))
                    .map(|_| destination)
                    .map_err(|error| error.to_string());
                self.send_event(Event::SavedAs { id, result })?;
                return Ok(());
            }
            Command::PrepareExternalDrag { id, token } => {
                let result = self
                    .history
                    .item(id)
                    .and_then(|item| crate::external_drag::prepare_external_drag(&item))
                    .map_err(|error| error.to_string());
                return self.send_event(Event::ExternalDragPrepared { id, token, result });
            }
            Command::Copy(id) => return self.handle_copy(id, None, CopyMode::Original),
            Command::CopyPlainText(id) => return self.handle_copy(id, None, CopyMode::PlainText),
            Command::CopyPlainTextForPaste(id, request_id) => {
                return self.handle_copy(id, Some(request_id), CopyMode::PlainText);
            }
            Command::CopyForPaste(id, request_id) => {
                return self.handle_copy(id, Some(request_id), CopyMode::Original);
            }
            Command::CopyPath(id) => return self.handle_copy(id, None, CopyMode::Paths),
            Command::CopyPathForPaste(id, request_id) => {
                return self.handle_copy(id, Some(request_id), CopyMode::Paths);
            }
            Command::CopyAsFile(id) => return self.handle_copy(id, None, CopyMode::Files),
            Command::CopyAsFileForPaste(id, request_id) => {
                return self.handle_copy(id, Some(request_id), CopyMode::Files);
            }
            Command::CopyAsFormat { id, format } => {
                return self.handle_copy(id, None, CopyMode::from(format));
            }
            Command::CopyAsFormatForPaste {
                id,
                format,
                request_id,
            } => {
                return self.handle_copy(id, Some(request_id), CopyMode::from(format));
            }
            Command::CopyTransformed { id, transform } => {
                return self.handle_copy(id, None, CopyMode::Transform(transform));
            }
            Command::CopyTransformedForPaste {
                id,
                transform,
                request_id,
            } => {
                return self.handle_copy(id, Some(request_id), CopyMode::Transform(transform));
            }
            Command::RestoreStashedClipboard {
                request_id,
                clipboard_sequence,
            } => return self.handle_restore_stash(request_id, clipboard_sequence),
            Command::CancelPaste { request_id } => {
                self.cancel_paste_stash(request_id);
                return Ok(());
            }
            Command::Preview { id, generation } => {
                return self.handle_preview(id, generation, false);
            }
            Command::HoverPreview { id, generation } => {
                return self.handle_preview(id, generation, true);
            }
            Command::EditContent { id, generation } => {
                let result = self
                    .history
                    .item(id)
                    .and_then(|item| {
                        let kind = item.kind().context("该类型记录不支持文本编辑")?;
                        anyhow::ensure!(kind.is_text_like(), "该类型记录不支持文本编辑");
                        let text = item.text_content.context("记录没有可编辑的纯文本")?;
                        Ok(if kind.is_rich() {
                            clipboard_core::PreviewContent::RichText(text)
                        } else {
                            clipboard_core::PreviewContent::Text(text)
                        })
                    })
                    .map_err(|error| error.to_string());
                return self.send_event(Event::Preview {
                    id,
                    generation,
                    result,
                });
            }
            Command::EditText {
                id,
                expected_hash,
                new_text,
                generation,
            } => {
                let result = self
                    .history
                    .edit_text(id, &expected_hash, &new_text, &self.images_dir)
                    .map_err(|error| error.to_string());
                if matches!(&result, Ok(true)) {
                    self.snapshot()?;
                }
                self.send_event(Event::TextEdited {
                    id,
                    generation,
                    result,
                })?;
                return Ok(());
            }
            Command::Delete(id) => {
                let outcome = self.history.delete_with_media(id, &self.images_dir)?;
                let refresh = self.snapshot().context("记录已删除，但列表刷新失败");
                self.send_event(Event::ItemDeleted { id, outcome })?;
                if let Err(error) = refresh {
                    self.send_event(Event::BackgroundError(error.to_string()))?;
                }
                return Ok(());
            }
            Command::DeleteBatch {
                ids,
                group_id,
                generation,
            } => return self.handle_delete_batch(ids, group_id, generation),
            Command::ClearHistory {
                group_id,
                generation,
            } => return self.handle_clear_history(group_id, generation),
            Command::ClearAllHistory => return self.handle_clear_all(),
            Command::ToggleFavorite(id) => {
                self.history.toggle_favorite(id)?;
            }
            Command::TogglePin(id) => {
                self.history.toggle_pin(id)?;
            }
            Command::BumpToTop(id) => {
                self.history.bump_to_top(id)?;
            }
            Command::ResolveQuickPaste {
                request_id,
                slot,
                favorite,
                group_id,
            } => {
                let result = self
                    .history
                    .quick_paste_item_id(slot, favorite, group_id)
                    .map_err(|error| error.to_string())
                    .and_then(|item| item.ok_or_else(|| format!("槽位 {slot} 没有可用的历史记录")));
                self.send_event(Event::QuickPasteResolved { request_id, result })?;
                return Ok(());
            }
            Command::Reorder {
                from,
                to,
                after,
                favorite_only,
                group_id,
                generation,
            } => {
                return self.handle_reorder(from, to, after, favorite_only, group_id, generation);
            }
            Command::CreateGroup(name) => return self.handle_create_group(name),
            Command::ReorderGroup { from, to, after } => {
                return self.handle_reorder_group(from, to, after);
            }
            Command::RenameGroup { id, name } => return self.handle_rename_group(id, name),
            Command::DeleteGroup { id, generation } => {
                return self.handle_delete_group(id, generation);
            }
            Command::MoveToGroup {
                id,
                source_group_id,
                target_group_id,
                generation,
            } => {
                return self.handle_move_to_group(id, source_group_id, target_group_id, generation);
            }
            Command::SetTheme(theme) => {
                return self.save_setting(theme, Preferences::set_theme, Event::ThemeSaved);
            }
            Command::SetCornerRadius(radius) => {
                return self.save_setting(
                    radius,
                    Preferences::set_corner_radius,
                    Event::CornerRadiusSaved,
                );
            }
            Command::SetLanguage(language) => {
                return self.save_setting(
                    language,
                    Preferences::set_language,
                    Event::LanguageSaved,
                );
            }
            Command::SetHotkey(hotkey) => return self.handle_set_hotkey(hotkey),
            Command::SetWindowSize(size) => {
                return self.save_setting(
                    size,
                    Preferences::set_window_size,
                    Event::WindowSizeSaved,
                );
            }
            Command::SetPersistWindowSize(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_persist_window_size,
                    Event::PersistWindowSizeSaved,
                );
            }
            Command::SetAutoResetState(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_auto_reset_state,
                    Event::AutoResetStateSaved,
                );
            }
            Command::SetSearchAutoFocus(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_search_auto_focus,
                    Event::SearchAutoFocusSaved,
                );
            }
            Command::SetSearchAutoClear(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_search_auto_clear,
                    Event::SearchAutoClearSaved,
                );
            }
            Command::SetSkipClearConfirm(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_skip_clear_confirm,
                    Event::SkipClearConfirmSaved,
                );
            }
            Command::SetPasteCloseWindow(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_paste_close_window,
                    Event::PasteCloseWindowSaved,
                );
            }
            Command::SetPasteKey(key) => {
                return self.save_setting(key, Preferences::set_paste_key, Event::PasteKeySaved);
            }
            Command::SetPasteMoveToTop(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_paste_move_to_top,
                    Event::PasteMoveToTopSaved,
                );
            }
            Command::SetQuickPasteEnabled(enabled) => {
                return self.save_setting(
                    enabled,
                    Preferences::set_quick_paste_enabled,
                    Event::QuickPasteEnabledSaved,
                );
            }
            Command::SetPasteShortcuts(shortcuts) => {
                return self.handle_set_paste_shortcuts(shortcuts);
            }
            Command::SetWindowPosition(position) => {
                return self.save_setting(
                    position,
                    Preferences::set_window_position,
                    Event::WindowPositionSaved,
                );
            }
            Command::SetHoverPreview(preference) => {
                return self.save_setting(
                    preference,
                    Preferences::set_hover_preview,
                    Event::HoverPreviewSaved,
                );
            }
            Command::SetDisplay(preference) => {
                return self.save_setting(
                    preference,
                    Preferences::set_display,
                    Event::DisplaySaved,
                );
            }
            Command::SetRetention { policy, confirmed } => {
                return self.handle_set_retention(policy, confirmed);
            }
            Command::SetAudio(preference) => {
                return self.save_setting(preference, Preferences::set_audio, Event::AudioSaved);
            }
            Command::SetMonitorTypes(preference) => {
                return self.handle_set_monitor_types(preference);
            }
            Command::SetTransientFilter(enabled) => {
                return self.handle_set_transient_filter(enabled);
            }
            Command::SetAppFilter(preference) => return self.handle_set_app_filter(preference),
            Command::SetWindowEffect(preference) => {
                return self.save_setting(
                    preference,
                    Preferences::set_window_effect,
                    Event::WindowEffectSaved,
                );
            }
            Command::SetDedupStrategy(strategy) => {
                return self.save_setting(
                    strategy,
                    Preferences::set_dedup_strategy,
                    Event::DedupStrategySaved,
                );
            }
            Command::SetRestoreClipboard(enabled) => {
                let result = self
                    .preferences
                    .set_restore_clipboard(enabled)
                    .map(|()| enabled)
                    .map_err(|error| error.to_string());
                if matches!(result, Ok(false)) {
                    self.clipboard_stash = None;
                }
                return self.send_event(Event::RestoreClipboardSaved(result));
            }
            Command::SetSequenceHotkey(shortcut) => {
                return self.handle_set_sequence_hotkey(shortcut);
            }
            Command::SetNote { id, note } => {
                let result = self
                    .history
                    .set_note(id, &note)
                    .map(|_| id)
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    self.snapshot()?;
                }
                self.send_event(Event::NoteSaved(result))?;
                return Ok(());
            }
            Command::ListRunningApps => {
                let apps = source_app::running_apps(&self.icons_dir);
                self.send_event(Event::RunningApps(apps))?;
                return Ok(());
            }
            Command::CompleteOnboarding => {
                let result = self
                    .preferences
                    .set_onboarding_completed()
                    .map_err(|error| error.to_string());
                self.send_event(Event::OnboardingCompleted(result))?;
                return Ok(());
            }
            Command::SetAutostart(enabled) => {
                let result = autostart::set_enabled(&self.data_dir, enabled)
                    .map(|_| enabled)
                    .map_err(|error| error.to_string());
                self.send_event(Event::AutostartSaved(result))?;
                return Ok(());
            }
            Command::QueryDataSize => {
                let result = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .map_err(|error| error.to_string());
                self.send_event(Event::DataSize(result))?;
                return Ok(());
            }
            Command::QueryDailyCounts(start_date) => {
                let result = self
                    .history
                    .daily_counts(&start_date)
                    .map_err(|error| error.to_string());
                self.send_event(Event::DailyCounts(result))?;
                return Ok(());
            }
            Command::OptimizeDatabase => {
                self.history.optimize_storage()?;
                let size = data_size_info(&self.data_dir, &self.images_dir, &self.staged_dir)
                    .context("数据库已整理，但刷新数据占用失败")?;
                self.send_event(Event::DatabaseOptimized(size))?;
                return Ok(());
            }
            Command::OpenDataDirectory => {
                open_data_directory(&self.data_dir)?;
                self.send_event(Event::Status("已打开数据目录".into()))?;
                return Ok(());
            }
            Command::ExportBackup(destination) => {
                self.send_event(Event::BackupExported(
                    self.history
                        .export_backup(&destination)
                        .map_err(|error| error.to_string()),
                ))?;
                return Ok(());
            }
            Command::Pause(paused) => return self.handle_pause(paused),
        }
        self.snapshot()
    }

    fn handle_preview(&mut self, id: i64, generation: u64, hover: bool) -> Result<()> {
        let result = self
            .history
            .preview_content(id)
            .map_err(|error| error.to_string());
        let event = if hover {
            Event::HoverPreview {
                id,
                generation,
                result,
            }
        } else {
            Event::Preview {
                id,
                generation,
                result,
            }
        };
        self.send_event(event)
    }
}
