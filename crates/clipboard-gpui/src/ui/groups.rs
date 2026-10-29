use super::*;

impl ClipboardView {
    pub(super) fn refresh_group_select(&self, window: &mut Window, cx: &mut Context<Self>) {
        let choice = self
            .history
            .group_id()
            .map_or(GroupChoice::Default, GroupChoice::Existing);
        let options = group_options(&self.groups, self.language, self.history.group_id());
        self.group_select.update(cx, |select, cx| {
            select.set_items(options, window, cx);
            select.set_selected_value(&choice, window, cx);
        });
    }

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
            cx.defer_in(window, |this, window, cx| {
                this.refresh_group_select(window, cx)
            });
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
            GroupChoice::Rename => {
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
            GroupChoice::Delete => {
                self.reset_selection();
                self.group_editor_open = false;
                self.group_rename_id = None;
                self.group_move_id = None;
                window.focus(&self.list_focus, cx);
                if let Some(id) = self.history.group_id() {
                    self.open_history_confirmation(
                        HistoryConfirmation::DeleteGroup(id),
                        window,
                        cx,
                    );
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
        cx.defer_in(window, |this, window, cx| {
            this.refresh_group_select(window, cx)
        });
        cx.notify();
    }

    pub(super) fn select_group(
        &mut self,
        group_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.group_id() == group_id {
            return;
        }
        self.cancel_sequence(cx);
        self.cancel_pending_row_click();
        self.close_hover_preview("group_changed", cx);
        self.search_task = None;
        self.group_move_id = None;
        self.close_history_confirmation(window, cx);
        self.reset_selection();
        self.history.set_group(group_id);
        cx.defer_in(window, |this, window, cx| {
            this.refresh_group_select(window, cx)
        });
        self.scroll_to_top();
        self.query(cx);
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
        self.group_edit_error = None;
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, window, cx| {
            let entity = owner.upgrade().expect("group dialog requires its owner");
            let view = entity.read(cx);
            let language = view.language;
            let renaming = view.group_rename_id.is_some();
            let pending = view.group_save_pending;
            let input = view.group_name_input.clone();
            let error = view.group_edit_error.clone();
            let keyboard_owner = owner.clone();
            let cancel_owner = owner.clone();
            let cancel_button_owner = owner.clone();
            let submit_owner = owner.clone();
            dialog
                .w(px(400.))
                .p_4()
                .rounded(cx.theme().radius_lg)
                .bg(cx.theme().background)
                .border_color(cx.theme().border)
                .title(
                    div()
                        .pb_3()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .text_size(px(16.))
                        .font_semibold()
                        .child(if renaming {
                            tr(language, "重命名分组", "Rename group")
                        } else {
                            tr(language, "新建分组", "Create group")
                        }),
                )
                .margin_top(((window.viewport_size().height - px(240.)) / 2.).max(px(16.)))
                .close_button(false)
                .overlay_closable(false)
                .on_ok(move |_, window, cx| {
                    let _ = keyboard_owner.update(cx, |view, cx| view.save_group(window, cx));
                    false
                })
                .on_cancel(move |_, window, cx| {
                    cancel_owner
                        .update(cx, |view, cx| view.cancel_group_edit(window, cx))
                        .unwrap_or(true)
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .pt_1()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_medium()
                                .text_color(cx.theme().muted_foreground)
                                .child(tr(language, "分组名称", "Group name")),
                        )
                        .child(Input::new(&input).text_size(px(14.)))
                        .when_some(error, |body, error| {
                            body.child(Alert::error("group-edit-error", error).small())
                        }),
                )
                .footer(
                    DialogFooter::new()
                        .pt_3()
                        .gap_2()
                        .border_t_1()
                        .border_color(cx.theme().border)
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
                        ),
                )
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
        self.group_rename_id = None;
        self.group_edit_error = None;
        self.group_name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        window.focus(&self.list_focus, cx);
        cx.notify();
        true
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
