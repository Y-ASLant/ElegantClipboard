use super::*;

impl ClipboardView {
    pub(super) fn choose_group(
        &mut self,
        choice: GroupChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_save_pending
            || self.group_delete_pending
            || self.clear_pending
            || self.batch_pending
            || self.group_reorder_pending
        {
            return;
        }
        match choice {
            GroupChoice::Default => self.select_group(None, window, cx),
            GroupChoice::Existing(id) => self.select_group(Some(id), window, cx),
            GroupChoice::Create => {
                self.group_rename_id = None;
                self.group_name_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.open_group_editor(window, cx);
            }
            GroupChoice::Edit => {
                if let Some(group) = self
                    .groups
                    .iter()
                    .find(|group| Some(group.id) == self.history.group_id())
                {
                    self.group_rename_id = Some(group.id);
                    self.group_name_input.update(cx, |input, cx| {
                        input.set_value(group.name.clone(), window, cx);
                    });
                    self.open_group_editor(window, cx);
                }
            }
            GroupChoice::MoveUp | GroupChoice::MoveDown => {
                if let Some(id) = self.history.group_id()
                    && let Some(index) = self.groups.iter().position(|group| group.id == id)
                    && let Some(target_index) =
                        index.checked_add_signed(if choice == GroupChoice::MoveUp { -1 } else { 1 })
                    && let Some(target) = self.groups.get(target_index)
                    && self.send(
                        Command::ReorderGroup {
                            from: id,
                            to: target.id,
                            after: choice == GroupChoice::MoveDown,
                        },
                        cx,
                    )
                {
                    self.group_reorder_pending = true;
                }
            }
        }
        cx.notify();
    }

    pub(super) fn select_group(
        &mut self,
        group_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.group_id() == group_id {
            if self.history_needs_reload() {
                self.scroll_to_top();
                self.reload_current_history(cx);
                window.focus(&self.list_focus, cx);
                cx.notify();
            }
            return;
        }
        self.release_history_drag(cx);
        self.cancel_sequence(cx);
        self.close_hover_preview("group_changed", cx);
        self.group_move_id = None;
        self.close_history_confirmation(window, cx);
        self.reset_selection();
        if let Some(reload) = self.history.set_group(group_id) {
            self.scroll_to_top();
            self.arm_history_query(reload, false, cx);
        }
        window.focus(&self.list_focus, cx);
        cx.notify();
    }
    pub(super) fn select_adjacent_group(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.group_delete_pending || self.clear_pending || self.group_reorder_pending {
            return;
        }
        let index = self
            .history
            .group_id()
            .and_then(|id| self.groups.iter().position(|group| group.id == id))
            .map_or(0, |index| index + 1);
        let Some(next) = index.checked_add_signed(direction) else {
            return;
        };
        if next == index || next > self.groups.len() {
            return;
        }
        self.select_group((next > 0).then(|| self.groups[next - 1].id), window, cx);
    }
    pub(super) fn open_group_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.close_editor(window, cx) {
            return;
        }
        self.group_editor_open = true;
        self.group_editor_confirming_delete = false;
        self.group_edit_error = None;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let entity = owner.upgrade().expect("group dialog requires its owner");
            let view = entity.read(cx);
            let language = view.language;
            let renaming = view.group_rename_id.is_some();
            let confirming_delete = view.group_editor_confirming_delete;
            let pending = view.group_save_pending || view.group_delete_pending;
            let input = view.group_name_input.clone();
            let error = view.group_edit_error.clone();
            let keyboard_owner = owner.clone();
            let cancel_owner = owner.clone();
            let cancel_button_owner = owner.clone();
            let submit_owner = owner.clone();
            let delete_owner = owner.clone();
            let back_owner = owner.clone();
            let confirm_delete_owner = owner.clone();
            let (title, prompt) = if confirming_delete {
                let (name, count) = view
                    .group_rename_id
                    .and_then(|id| view.groups.iter().find(|group| group.id == id))
                    .map(|group| (group.name.as_str(), group.item_count))
                    .unwrap_or((tr(language, "该分组", "this group"), 0));
                let prompt = if language == LanguagePreference::English {
                    format!("Delete “{name}”? {count} items will move to the default group.")
                } else {
                    format!("删除「{name}」？{count} 条记录将移到默认分组。")
                };
                (tr(language, "删除分组", "Delete group"), prompt)
            } else {
                (
                    tr(
                        language,
                        if renaming {
                            "编辑分组"
                        } else {
                            "新建分组"
                        },
                        if renaming {
                            "Edit group"
                        } else {
                            "Create group"
                        },
                    ),
                    String::new(),
                )
            };
            dialog_shell(dialog, cx)
                .w(px(visual::DIALOG_WIDTH_COMPACT))
                .title(dialog_title(title, cx))
                .on_ok(move |_, window, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| {
                        if view.group_editor_confirming_delete {
                            view.confirm_group_delete(window, cx);
                        } else {
                            view.save_group(window, cx);
                        }
                    });
                    false
                })
                .on_cancel(move |_, window, cx| {
                    cancel_owner
                        .update(cx, |view, cx| view.cancel_group_edit(window, cx))
                        .unwrap_or(true)
                })
                .child(if confirming_delete {
                    dialog_copy()
                        .flex()
                        .flex_col()
                        .pt_1()
                        .gap_2()
                        .child(prompt)
                        .when_some(error, |body, error| {
                            body.child(Alert::error("group-edit-error", error).small())
                        })
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .pt_1()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(visual::TEXT_SMALL))
                                .font_medium()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(language, "分组名称", "Group name")),
                        )
                        .child(Input::new(&input).text_size(px(visual::TEXT_BODY)))
                        .when_some(error, |body, error| {
                            body.child(Alert::error("group-edit-error", error).small())
                        })
                        .into_any_element()
                })
                .footer(if confirming_delete {
                    dialog_footer(cx)
                        .child(
                            Button::new("group-delete-back")
                                .outline()
                                .label(tr(language, "返回", "Back"))
                                .disabled(view.group_delete_pending)
                                .on_click(move |_, _, cx| {
                                    let _ = back_owner.update(cx, |view, cx| {
                                        view.group_editor_confirming_delete = false;
                                        view.group_edit_error = None;
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            Button::new("group-delete-confirm")
                                .danger()
                                .label(if view.group_delete_pending {
                                    tr(language, "正在删除分组…", "Deleting group…")
                                } else {
                                    tr(
                                        language,
                                        "保留记录并删除分组",
                                        "Delete group and keep items",
                                    )
                                })
                                .disabled(view.group_delete_pending)
                                .on_click(move |_, window, cx| {
                                    let _ = confirm_delete_owner.update(cx, |view, cx| {
                                        view.confirm_group_delete(window, cx);
                                    });
                                }),
                        )
                } else {
                    dialog_footer(cx)
                        .when(renaming, |footer| {
                            footer.child(
                                Button::new("group-edit-delete")
                                    .danger()
                                    .mr_auto()
                                    .label(tr(language, "删除分组", "Delete group"))
                                    .disabled(pending)
                                    .on_click(move |_, window, cx| {
                                        let _ = delete_owner.update(cx, |view, cx| {
                                            view.request_group_delete(window, cx);
                                        });
                                    }),
                            )
                        })
                        .child(
                            Button::new("group-save-cancel")
                                .outline()
                                .label(tr(language, "取消", "Cancel"))
                                .disabled(pending)
                                .on_click(move |_, window, cx| {
                                    if cancel_button_owner
                                        .update(cx, |view, cx| view.cancel_group_edit(window, cx))
                                        .unwrap_or(false)
                                    {
                                        window.close_dialog(cx);
                                    }
                                }),
                        )
                        .child(
                            Button::new("group-save-submit")
                                .primary()
                                .label(if renaming {
                                    tr(language, "保存名称", "Save name")
                                } else {
                                    tr(language, "创建", "Create")
                                })
                                .disabled(pending)
                                .on_click(move |_, window, cx| {
                                    let _ = submit_owner
                                        .update(cx, |view, cx| view.save_group(window, cx));
                                }),
                        )
                })
        });
        cx.defer_in(window, |view, window, cx| {
            view.group_name_input
                .update(cx, |input, cx| input.focus(window, cx));
        });
        cx.notify();
    }

    pub(super) fn cancel_group_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.group_save_pending {
            return false;
        }
        self.group_editor_open = false;
        self.group_editor_confirming_delete = false;
        self.group_rename_id = None;
        self.group_edit_error = None;
        self.group_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.list_focus, cx);
        cx.notify();
        true
    }

    /// 编辑弹窗内点「删除分组」：同一弹窗切到确认态，不再关旧开新。
    fn request_group_delete(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.group_rename_id.is_none() || self.group_delete_pending || self.group_save_pending {
            return;
        }
        self.group_editor_confirming_delete = true;
        self.group_edit_error = None;
        cx.notify();
    }

    /// 确认态里点「确认删除」或按 Enter：直接发删除命令，复用原删除链路。
    fn confirm_group_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.group_rename_id else {
            return;
        };
        if self.group_delete_pending {
            return;
        }
        if self.send(
            Command::DeleteGroup {
                id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.group_delete_pending = true;
            cx.notify();
        } else {
            self.group_edit_error = Some(self.message.clone());
            window.refresh();
            cx.notify();
        }
    }

    pub(super) fn save_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.group_save_pending {
            return;
        }
        let name = self.group_name_input.read(cx).value().to_string();
        let command = match self.group_rename_id {
            Some(id) => Command::RenameGroup { id, name },
            None => Command::CreateGroup(name),
        };
        if self.send(command, cx) {
            self.group_save_pending = true;
            self.group_edit_error = None;
        } else {
            self.group_edit_error = Some(self.message.clone());
        }
        window.refresh();
        cx.notify();
    }

    pub(super) fn delete_group(&mut self, cx: &mut Context<Self>) {
        if self.group_delete_pending {
            return;
        }
        let Some(HistoryConfirmation::DeleteGroup(id)) = self.confirmation else {
            return;
        };
        if !self.confirmation_active(HistoryConfirmation::DeleteGroup(id)) {
            return;
        }
        if self.send(
            Command::DeleteGroup {
                id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.group_delete_pending = true;
            cx.notify();
        }
    }
    pub(super) fn move_to_group(
        &mut self,
        id: i64,
        target_group_id: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        if self.group_move_pending || target_group_id == self.history.group_id() {
            return;
        }
        if self.send(
            Command::MoveToGroup {
                id,
                source_group_id: self.history.group_id(),
                target_group_id,
                generation: self.history.generation,
            },
            cx,
        ) {
            self.group_move_pending = true;
            cx.notify();
        }
    }
}
