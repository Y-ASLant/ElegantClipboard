use super::*;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
pub(super) fn history_menu_item(
    label: &'static str,
    disabled: bool,
    view: Entity<ClipboardView>,
    id: i64,
    action: HistoryMenuAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            view.update(cx, |this, cx| {
                this.perform_history_menu_action(id, &action, window, cx);
            });
        })
}

impl ClipboardView {
    pub(super) fn perform_history_menu_action(
        &mut self,
        id: i64,
        action: &HistoryMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history.loading
            || self.batch_pending
            || self.batch_mode
            || !self.history.items.iter().any(|item| item.id == id)
        {
            return;
        }
        self.cancel_pending_row_click();
        self.history.selected = Some(id);
        match action {
            HistoryMenuAction::Paste => self.paste_selected(id, window, cx),
            HistoryMenuAction::PastePlainText => self.copy_or_paste_plain_text(id, window, cx),
            HistoryMenuAction::Copy => {
                self.send(Command::Copy(id), cx);
            }
            HistoryMenuAction::CopyPath => self.copy_or_paste_path(id, window, cx),
            HistoryMenuAction::Preview => self.open_preview(id, window, cx),
            HistoryMenuAction::Edit => self.open_preview_for_edit(id, window, cx),
            HistoryMenuAction::Reveal => {
                self.send(Command::RevealInExplorer(id), cx);
            }
            HistoryMenuAction::SaveAs(name) => self.start_save_as(id, name.clone(), cx),
            HistoryMenuAction::ToggleFavorite => {
                self.send(Command::ToggleFavorite(id), cx);
            }
            HistoryMenuAction::TogglePin => {
                self.send(Command::TogglePin(id), cx);
            }
            HistoryMenuAction::MoveToGroup => {
                if !self.group_move_pending {
                    self.group_move_id = Some(id);
                }
            }
            HistoryMenuAction::Delete => {
                self.send(Command::Delete(id), cx);
            }
        }
        cx.notify();
    }
    pub(super) fn history_row_menu(
        &self,
        row: Stateful<Div>,
        item: &DbClipboardItem,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = item.id;
        let is_image = item.content_type == "image";
        let is_files = item.content_type == "files";
        let favorite = item.is_favorite;
        let pinned = item.is_pinned;
        let entity = cx.entity();
        let text_like = matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf");
        let language = self.language;
        let batch_mode = self.batch_mode;
        let can_paste = self.paste_target.is_some() && self._tray.is_some();
        let paste_pending = self.paste_pending.is_some();
        let save_as_pending = self.save_as_pending.is_some();
        let group_move_disabled = self.group_move_pending
            || (self.groups.is_empty() && self.history.group_id().is_none());
        let save_as_name = if is_image {
            item.image_path.as_deref().and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
        } else if is_files {
            item.file_paths
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
                .and_then(|paths| paths.into_iter().next())
                .and_then(|path| {
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                })
        } else {
            None
        };
        if batch_mode {
            row.into_any_element()
        } else {
            row.context_menu(move |menu, _, _| {
                let menu = menu
                    .item(history_menu_item(
                        tr(language, "粘贴到原窗口", "Paste to previous window"),
                        !can_paste || paste_pending,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Paste,
                    ))
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            if can_paste {
                                tr(language, "粘贴纯文本", "Paste plain text")
                            } else {
                                tr(language, "复制纯文本", "Copy plain text")
                            },
                            paste_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::PastePlainText,
                        ))
                    })
                    .item(history_menu_item(
                        tr(language, "复制", "Copy"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Copy,
                    ));
                let menu = if is_image || is_files {
                    let menu = menu.item(history_menu_item(
                        if can_paste {
                            tr(language, "粘贴路径", "Paste paths")
                        } else {
                            tr(language, "复制路径", "Copy paths")
                        },
                        paste_pending,
                        entity.clone(),
                        id,
                        HistoryMenuAction::CopyPath,
                    ));
                    let menu = menu.item(history_menu_item(
                        tr(language, "在资源管理器中显示", "Show in File Explorer"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Reveal,
                    ));
                    if let Some(name) = save_as_name.clone() {
                        menu.item(history_menu_item(
                            tr(language, "另存为", "Save as"),
                            save_as_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::SaveAs(name),
                        ))
                    } else {
                        menu
                    }
                } else {
                    menu
                };
                menu.separator()
                    .item(history_menu_item(
                        tr(language, "查看详情", "View details"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Preview,
                    ))
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            tr(language, "编辑", "Edit"),
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::Edit,
                        ))
                    })
                    .separator()
                    .item(history_menu_item(
                        if favorite {
                            tr(language, "取消收藏", "Unfavorite")
                        } else {
                            tr(language, "收藏", "Favorite")
                        },
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::ToggleFavorite,
                    ))
                    .item(history_menu_item(
                        if pinned {
                            tr(language, "取消置顶", "Unpin")
                        } else {
                            tr(language, "置顶", "Pin")
                        },
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::TogglePin,
                    ))
                    .item(history_menu_item(
                        tr(language, "移动到分组", "Move to group"),
                        group_move_disabled,
                        entity.clone(),
                        id,
                        HistoryMenuAction::MoveToGroup,
                    ))
                    .separator()
                    .item(history_menu_item(
                        tr(language, "删除", "Delete"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Delete,
                    ))
            })
            .into_any_element()
        }
    }
}
