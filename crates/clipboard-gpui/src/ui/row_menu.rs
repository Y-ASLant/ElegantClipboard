use super::file_cards::{copy_or_paste_label, save_as_suggested_name};
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
pub(super) fn history_menu_item(
    label: impl Into<SharedString>,
    disabled: bool,
    view: Entity<ClipboardView>,
    id: i64,
    action: HistoryMenuAction,
) -> PopupMenuItem {
    let label = label.into();
    let icon = match &action {
        HistoryMenuAction::Paste => IconName::Replace,
        HistoryMenuAction::PastePlainText
        | HistoryMenuAction::PasteAsFormat(_)
        | HistoryMenuAction::PasteTransformed(_)
        | HistoryMenuAction::Edit
        | HistoryMenuAction::EditNote => IconName::FileText,
        HistoryMenuAction::Copy | HistoryMenuAction::CopyPath => IconName::Copy,
        HistoryMenuAction::CopyAsFile | HistoryMenuAction::SaveAs(_) => IconName::File,
        HistoryMenuAction::Reveal => IconName::ExternalLink,
        HistoryMenuAction::ToggleFavorite => IconName::Heart,
        HistoryMenuAction::TogglePin => IconName::Star,
        HistoryMenuAction::MoveToGroup => IconName::Folder,
        HistoryMenuAction::Delete => IconName::Trash,
    };
    PopupMenuItem::new(label)
        .icon(Icon::new(icon).size(px(visual::ICON_SIZE)))
        .disabled(disabled)
        .on_click(move |_, window, cx| {
            view.update(cx, |this, cx| {
                this.perform_history_menu_action(id, &action, window, cx);
            });
        })
}

/// 「粘贴为/复制为」子菜单的公共构造：逐项列出 (标签, 动作)，
/// 样式与点击行为在两个子菜单（按格式/转换后）之间共享。
#[allow(clippy::too_many_arguments)]
fn transform_submenu(
    menu: PopupMenu,
    language: LanguagePreference,
    can_paste: bool,
    paste_pending: bool,
    id: i64,
    entity: &Entity<ClipboardView>,
    items: impl IntoIterator<Item = (&'static str, HistoryMenuAction)>,
) -> PopupMenu {
    let mut menu = menu.min_w(px(216.)).max_w(px(320.));
    let (menu_label, menu_label_en) = if can_paste {
        ("粘贴为", "Paste as")
    } else {
        ("复制为", "Copy as")
    };
    for (label, action) in items {
        menu = menu.item(
            PopupMenuItem::new(format!(
                "{} {label}",
                tr(language, menu_label, menu_label_en)
            ))
            .disabled(paste_pending)
            .on_click({
                let entity = entity.clone();
                move |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        this.perform_history_menu_action(id, &action, window, cx);
                    });
                }
            }),
        );
    }
    menu
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
            HistoryMenuAction::PasteAsFormat(format) => {
                self.copy_or_paste_format(id, *format, window, cx)
            }
            HistoryMenuAction::PasteTransformed(transform) => {
                self.copy_or_paste_transform(id, *transform, window, cx)
            }
            HistoryMenuAction::Copy => {
                self.send(Command::Copy(id), cx);
            }
            HistoryMenuAction::CopyPath => self.copy_or_paste_path(id, window, cx),
            HistoryMenuAction::CopyAsFile => self.copy_or_paste_image_file(id, window, cx),
            HistoryMenuAction::Edit => self.open_editor(id, true, window, cx),
            HistoryMenuAction::EditNote => self.open_editor(id, false, window, cx),
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
        let is_rich_item = item.kind().is_some_and(|kind| kind.is_rich());
        let is_text_item = matches!(item.content_type.as_str(), "text" | "url");
        let favorite = item.is_favorite;
        let pinned = item.is_pinned;
        let entity = cx.entity();
        let text_like = item.kind().is_some_and(|kind| kind.is_text_like());
        let language = self.language;
        let batch_mode = self.batch_mode;
        let can_paste = self.paste_target.is_some() && self._tray.is_some();
        let paste_pending = self.paste_pending.is_some();
        let save_as_pending = self.save_as_pending.is_some();
        let group_move_disabled = self.group_move_disabled();
        let save_as_name = if is_image {
            item.image_path.as_deref().and_then(save_as_suggested_name)
        } else if is_files {
            clipboard_core::parse_file_paths_for_display(item.file_paths.as_deref())
                .into_iter()
                .next()
                .and_then(save_as_suggested_name)
        } else {
            None
        };
        if batch_mode {
            row.into_any_element()
        } else {
            row.context_menu(move |menu, window, cx| {
                let menu = menu
                    .min_w(px(240.))
                    .max_w(px(320.))
                    .scrollable(true)
                    .item(history_menu_item(
                        tr(language, "粘贴到原窗口", "Paste to previous window"),
                        !can_paste || paste_pending,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Paste,
                    ))
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            copy_or_paste_label(language, can_paste, "纯文本", "plain text"),
                            paste_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::PastePlainText,
                        ))
                    })
                    // 富文本条目：按单一目标格式复制/粘贴的子菜单。
                    .when(is_rich_item, |menu| {
                        let submenu =
                            |menu: PopupMenu,
                             _window: &mut Window,
                             _cx: &mut Context<PopupMenu>| {
                                transform_submenu(
                                    menu,
                                    language,
                                    can_paste,
                                    paste_pending,
                                    id,
                                    &entity,
                                    [
                                        (
                                            tr(language, "HTML", "HTML"),
                                            HistoryMenuAction::PasteAsFormat(CopyFormat::Html),
                                        ),
                                        (
                                            tr(language, "RTF", "RTF"),
                                            HistoryMenuAction::PasteAsFormat(CopyFormat::Rtf),
                                        ),
                                    ],
                                )
                            };
                        menu.item(PopupMenuItem::submenu(
                            if can_paste {
                                tr(language, "按格式粘贴", "Paste as format")
                            } else {
                                tr(language, "按格式复制", "Copy as format")
                            },
                            PopupMenu::build(window, cx, submenu),
                        ))
                    })
                    // 文本条目：转换后的复制/粘贴子菜单。
                    .when(is_text_item, |menu| {
                        let submenu =
                            |menu: PopupMenu,
                             _window: &mut Window,
                             _cx: &mut Context<PopupMenu>| {
                                transform_submenu(
                                    menu,
                                    language,
                                    can_paste,
                                    paste_pending,
                                    id,
                                    &entity,
                                    [
                                        (
                                            tr(language, "去除首尾空白", "Trim whitespace"),
                                            HistoryMenuAction::PasteTransformed(
                                                TextTransform::Trim,
                                            ),
                                        ),
                                        (
                                            tr(language, "去除换行", "Remove line breaks"),
                                            HistoryMenuAction::PasteTransformed(
                                                TextTransform::RemoveLineBreaks,
                                            ),
                                        ),
                                        (
                                            tr(language, "转大写", "UPPERCASE"),
                                            HistoryMenuAction::PasteTransformed(
                                                TextTransform::Uppercase,
                                            ),
                                        ),
                                        (
                                            tr(language, "转小写", "lowercase"),
                                            HistoryMenuAction::PasteTransformed(
                                                TextTransform::Lowercase,
                                            ),
                                        ),
                                    ],
                                )
                            };
                        menu.item(PopupMenuItem::submenu(
                            if can_paste {
                                tr(language, "转换后粘贴", "Paste transformed")
                            } else {
                                tr(language, "转换后复制", "Copy transformed")
                            },
                            PopupMenu::build(window, cx, submenu),
                        ))
                    })
                    .item(history_menu_item(
                        tr(language, "复制", "Copy"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::Copy,
                    ))
                    .when(is_image, |menu| {
                        menu.item(history_menu_item(
                            if can_paste {
                                tr(language, "作为文件粘贴", "Paste as file")
                            } else {
                                tr(language, "作为文件复制", "Copy as file")
                            },
                            paste_pending,
                            entity.clone(),
                            id,
                            HistoryMenuAction::CopyAsFile,
                        ))
                    });
                let menu = if is_image || is_files {
                    let menu = menu.item(history_menu_item(
                        copy_or_paste_label(language, can_paste, "路径", "paths"),
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
                    .when(text_like, |menu| {
                        menu.item(history_menu_item(
                            tr(language, "编辑", "Edit"),
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::Edit,
                        ))
                    })
                    .item(history_menu_item(
                        tr(language, "编辑备注", "Edit note"),
                        false,
                        entity.clone(),
                        id,
                        HistoryMenuAction::EditNote,
                    ))
                    .separator()
                    .item(
                        history_menu_item(
                            if favorite {
                                tr(language, "取消收藏", "Unfavorite")
                            } else {
                                tr(language, "收藏", "Favorite")
                            },
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::ToggleFavorite,
                        )
                        .checked(favorite),
                    )
                    .item(
                        history_menu_item(
                            if pinned {
                                tr(language, "取消置顶", "Unpin")
                            } else {
                                tr(language, "置顶", "Pin")
                            },
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::TogglePin,
                        )
                        .checked(pinned),
                    )
                    .item(history_menu_item(
                        tr(language, "移动到分组", "Move to group"),
                        group_move_disabled,
                        entity.clone(),
                        id,
                        HistoryMenuAction::MoveToGroup,
                    ))
                    .separator()
                    .item(
                        history_menu_item(
                            tr(language, "删除", "Delete"),
                            false,
                            entity.clone(),
                            id,
                            HistoryMenuAction::Delete,
                        )
                        .icon(
                            Icon::new(IconName::Trash)
                                .size(px(visual::ICON_SIZE))
                                .text_color(cx.theme().danger),
                        ),
                    )
            })
            .into_any_element()
        }
    }
}
