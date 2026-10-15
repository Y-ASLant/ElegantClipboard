use super::*;

impl ClipboardView {
    /// 以下 `apply_*` 为 `apply_event` 按事件域拆分的处理函数；
    /// 返回 `false` 表示该事件在此提前结束（跳过末尾的确认弹窗同步）。
    pub(super) fn apply_groups(
        &mut self,
        groups: Vec<Group>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.history.group_id().is_some()
            && !groups
                .iter()
                .any(|group| Some(group.id) == self.history.group_id())
        {
            self.reset_selection();
            self.history.set_group(None);
            self.scroll_to_top();
            self.query(cx);
        }
        self.groups = groups;
        self.refresh_group_select(window, cx);
        true
    }

    pub(super) fn apply_group_reordered(&mut self, result: Result<(), String>) -> bool {
        self.group_reorder_pending = false;
        match result {
            Ok(()) => {
                self.set_feedback_message(
                    tr(self.language, "分组顺序已保存", "Group order saved").into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "分组排序失败", "Failed to reorder groups")
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_group_created(
        &mut self,
        result: Result<Group, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.group_save_pending = false;
        match result {
            Ok(group) => {
                self.group_editor_open = false;
                self.group_rename_id = None;
                self.group_edit_error = None;
                window.close_dialog(cx);
                self.group_name_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.select_group(Some(group.id), window, cx);
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Group created: {}", group.name)
                } else {
                    format!("已创建分组：{}", group.name)
                });
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "创建分组失败", "Failed to create group")
                ));
                self.is_error = true;
                self.group_edit_error = Some(self.message.clone());
                window.refresh();
            }
        }
        true
    }

    pub(super) fn apply_group_renamed(
        &mut self,
        result: Result<Group, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.group_save_pending = false;
        match result {
            Ok(group) => {
                self.group_editor_open = false;
                self.group_rename_id = None;
                self.group_edit_error = None;
                window.close_dialog(cx);
                self.group_name_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                window.focus(&self.list_focus, cx);
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Group renamed to: {}", group.name)
                } else {
                    format!("分组已重命名为：{}", group.name)
                });
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "重命名分组失败", "Failed to rename group")
                ));
                self.is_error = true;
                self.group_edit_error = Some(self.message.clone());
                window.refresh();
            }
        }
        true
    }

    pub(super) fn apply_group_deleted(
        &mut self,
        result: Result<usize, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.group_delete_pending = false;
        if matches!(self.confirmation, Some(HistoryConfirmation::DeleteGroup(_))) {
            self.close_history_confirmation(window, cx);
        }
        match result {
            Ok(count) => {
                window.focus(&self.list_focus, cx);
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Group deleted; {count} items moved to the default group")
                } else {
                    format!("已删除分组，{count} 条记录移至默认分组")
                });
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "删除分组失败", "Failed to delete group")
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_item_deleted(&mut self, outcome: clipboard_core::DeletionOutcome) -> bool {
        if outcome.cleanup_warning.is_some() {
            self.set_feedback_message(tr(self.language, "记录已删除", "Item deleted").into());
            self.apply_cleanup_warning(outcome.cleanup_warning);
        }
        true
    }

    fn apply_cleanup_warning(&mut self, warning: Option<String>) {
        self.is_error = warning.is_some();
        if let Some(warning) = warning {
            self.set_feedback_message(format!(
                "{}；{}: {warning}",
                self.message,
                tr(
                    self.language,
                    "部分附件未能回收",
                    "Some attachments could not be removed",
                )
            ));
        }
    }

    pub(super) fn apply_history_cleared(
        &mut self,
        result: Result<clipboard_core::DeletionOutcome, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.clear_pending = false;
        if matches!(
            self.confirmation,
            Some(HistoryConfirmation::ClearHistory(_))
        ) {
            self.close_history_confirmation(window, cx);
        }
        match result {
            Ok(outcome) => {
                let count = outcome.deleted;
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Cleared {count} unpinned, non-favorite items")
                } else {
                    format!("已清理 {count} 条未置顶且未收藏的记录")
                });
                self.apply_cleanup_warning(outcome.cleanup_warning);
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "清理历史失败", "Failed to clear history")
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_all_history_cleared(
        &mut self,
        result: Result<clipboard_core::DeletionOutcome, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.clear_all_pending = false;
        match result {
            Ok(outcome) => {
                let count = outcome.deleted;
                self.reset_selection();
                self.group_move_id = None;
                self.preview.close();
                self.preview_source_hash = None;
                self.preview_editing = false;
                self.preview_edit_requested = false;
                self.preview_save_pending = false;
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Deleted all {count} items; settings and groups were kept")
                } else {
                    format!("已删除全部 {count} 条历史，设置和分组已保留")
                });
                self.apply_cleanup_warning(outcome.cleanup_warning);
                self.clear_all_error = None;
                if let Some(handle) = self.settings_window {
                    cx.spawn(async move |_, cx| {
                        cx.update(|cx| {
                            let _ = handle.update(cx, |_, window, cx| window.close_dialog(cx));
                        });
                    })
                    .detach();
                }
                self.refresh_daily_counts(cx);
                window.focus(&self.list_focus, cx);
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "删除全部历史失败",
                        "Failed to delete all history"
                    )
                ));
                self.is_error = true;
                self.clear_all_error = Some(self.message.clone());
                cx.refresh_windows();
            }
        }
        true
    }

    pub(super) fn apply_batch_deleted(
        &mut self,
        result: Result<clipboard_core::DeletionOutcome, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.batch_pending = false;
        if self.confirmation == Some(HistoryConfirmation::DeleteSelected) {
            self.close_history_confirmation(window, cx);
        }
        match result {
            Ok(outcome) => {
                let count = outcome.deleted;
                self.reset_selection();
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Deleted {count} selected items")
                } else {
                    format!("已删除 {count} 条选中记录")
                });
                self.apply_cleanup_warning(outcome.cleanup_warning);
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "批量删除失败",
                        "Failed to delete selected items"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_item_moved(&mut self, result: Result<(), String>) -> bool {
        self.group_move_pending = false;
        match result {
            Ok(()) => {
                self.group_move_id = None;
                self.set_feedback_message(
                    tr(
                        self.language,
                        "已移动到目标分组",
                        "Moved to the selected group",
                    )
                    .into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "移动分组失败", "Failed to move item")
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_snapshot(
        &mut self,
        items: Vec<DbClipboardItem>,
        total: i64,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.external_drag.is_some() && generation == self.history.generation {
            self.drag_snapshot = Some((items, total, generation));
            return true;
        }
        if !self.history.apply(items, total, generation) {
            return true;
        }
        let loaded_ids: HashSet<_> = self.history.items.iter().map(|item| item.id).collect();
        let (top_index, partial) = history_top_row(&self.row_offsets, self.scroll.offset().y);
        self.rebuild_row_layout();
        let new_top = self.row_offsets[top_index.min(self.history.items.len())];
        self.scroll.set_offset(point(px(0.), -(new_top + partial)));
        if self
            .hover_preview
            .id
            .is_some_and(|id| !loaded_ids.contains(&id))
        {
            self.close_hover_preview(cx);
        }
        self.refresh_file_card_info(cx);
        self.sync_image_assets(cx);
        self.selected_ids.retain(|id| loaded_ids.contains(id));
        if self
            .selection_anchor
            .is_some_and(|id| !loaded_ids.contains(&id))
        {
            self.selection_anchor = None;
        }
        if !self.group_move_pending
            && self
                .group_move_id
                .is_some_and(|id| !loaded_ids.contains(&id))
        {
            self.group_move_id = None;
        }
        true
    }
}
