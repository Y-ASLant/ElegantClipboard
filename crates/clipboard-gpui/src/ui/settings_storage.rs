use super::settings_common::{owner_state, settings_description, settings_inline, settings_stack};
use super::*;
use gpui_kit::component::{alert::Alert, dialog::DialogFooter};

impl SettingsWindowView {
    pub(super) fn settings_storage_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let data_size = owner_state.data_size;
        let data_size_pending = owner_state.data_size_pending;
        let maintenance_pending = owner_state.database_maintenance_pending;
        let export_pending = owner_state.export_pending;
        let data_size_detail = data_size.map_or_else(
            || {
                tr(
                    language,
                    "尚未统计数据占用",
                    "Usage has not been calculated",
                )
                .to_owned()
            },
            |size| {
                let detail = if language == LanguagePreference::English {
                    format!(
                        "Total {} · Database {} · Images {} / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes)
                    )
                } else {
                    format!(
                        "共 {} · 数据库 {} · 图片 {} 个 / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes)
                    )
                };
                if size.staged_count == 0 && size.staged_bytes == 0 {
                    detail
                } else if language == LanguagePreference::English {
                    format!(
                        "{detail} · Legacy file copies pending cleanup {} / {}",
                        size.staged_count,
                        format_bytes(size.staged_bytes)
                    )
                } else {
                    format!(
                        "{detail} · 待清理旧文件副本 {} 个 / {}",
                        size.staged_count,
                        format_bytes(size.staged_bytes)
                    )
                }
            },
        );
        let refresh_owner = self.owner.clone();
        let optimize_owner = self.owner.clone();
        let folder_owner = self.owner.clone();
        let export_owner = self.owner.clone();

        settings_inline()
            .child(
                settings_description(data_size_detail, cx)
                    .w_auto()
                    .flex_basis(px(360.))
                    .flex_grow(1.),
            )
            .child(
                settings_inline()
                    .w_auto()
                    .child(
                        Button::new("settings-window-refresh")
                            .outline()
                            .icon(IconName::RotateCw)
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(tr(language, "刷新", "Refresh"))
                            .tooltip(tr(language, "刷新占用", "Refresh usage"))
                            .accessibility_label(tr(language, "刷新占用", "Refresh usage"))
                            .disabled(data_size_pending || maintenance_pending)
                            .on_click(move |_, _, cx| {
                                let _ = refresh_owner.update(cx, |owner, cx| {
                                    owner.refresh_data_size(cx);
                                    owner.refresh_daily_counts(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("settings-window-optimize")
                            .outline()
                            .icon(IconName::HardDrive)
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(tr(language, "整理", "Optimize"))
                            .tooltip(tr(language, "整理数据库", "Optimize database"))
                            .accessibility_label(tr(language, "整理数据库", "Optimize database"))
                            .disabled(data_size_pending || maintenance_pending)
                            .on_click(move |_, _, cx| {
                                let _ = optimize_owner.update(cx, |owner, cx| {
                                    if owner.send(Command::OptimizeDatabase, cx) {
                                        owner.database_maintenance_pending = true;
                                        cx.notify();
                                    }
                                });
                            }),
                    )
                    .child(
                        Button::new("settings-window-folder")
                            .outline()
                            .icon(IconName::FolderOpen)
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(tr(language, "打开目录", "Open folder"))
                            .tooltip(tr(language, "打开数据目录", "Open data folder"))
                            .accessibility_label(tr(language, "打开数据目录", "Open data folder"))
                            .on_click(move |_, _, cx| {
                                let _ = folder_owner.update(cx, |owner, cx| {
                                    owner.send(Command::OpenDataDirectory, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("settings-window-export")
                            .outline()
                            .icon(IconName::ExternalLink)
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(tr(language, "导出备份", "Export backup"))
                            .tooltip(tr(language, "导出备份", "Export backup"))
                            .accessibility_label(tr(language, "导出备份", "Export backup"))
                            .disabled(export_pending)
                            .on_click(move |_, _, cx| {
                                let _ = export_owner.update(cx, |owner, cx| {
                                    owner.start_export(cx);
                                });
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn settings_import_content(&self, cx: &mut Context<Self>) -> AnyElement {
        use data_import::ImportSource;
        let Some(owner) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner.language;
        let pending = owner.data_import_pending;
        let outcome = owner.data_import_result.as_ref();
        let open_owner = self.owner.clone();
        let choices = [
            (
                "import-legacy-db",
                tr(language, "导入旧版数据库", "Import legacy database"),
                ImportSource::LegacyDatabase,
            ),
            (
                "import-legacy-zip",
                tr(language, "导入旧版 ZIP", "Import legacy ZIP"),
                ImportSource::LegacyBackup,
            ),
            (
                "restore-backup",
                tr(language, "恢复备份", "Restore backup"),
                ImportSource::Backup,
            ),
        ];
        settings_stack()
            .child(settings_description(tr(
                language,
                "先选择来源文件，再选择空目标目录。不会覆盖、合并或切换当前数据；完成后可打开恢复的数据。旧版数据库仅复制记录，迁移附件请使用旧版 ZIP。",
                "Select a source file, then an empty destination folder. Current data will not be overwritten, merged or switched. Open the restored data when ready. A legacy database copies records only; use a legacy ZIP to migrate attachments.",
            ), cx))
            .child(
                settings_inline().children(choices.into_iter().map(|(id, label, source)| {
                    let owner = self.owner.clone();
                    Button::new(id).outline().h(px(32.)).label(label).disabled(pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| owner.start_data_import(source, cx));
                        })
                })),
            )
            .when(pending, |content| {
                content.child(settings_description(tr(language, "正在选择路径或恢复数据…", "Selecting paths or restoring data…"), cx))
            })
            .when_some(outcome, |content, outcome| {
                content
                    .child(settings_description(outcome.description(language), cx))
                    .child(
                        settings_inline().child(
                            Button::new("open-restored-data").outline().h(px(32.))
                                .label(tr(language, "打开恢复的数据", "Open restored data"))
                                .disabled(pending)
                                .on_click(move |_, _, cx| {
                                    let _ = open_owner.update(cx, |owner, cx| owner.open_imported_data(cx));
                                }),
                        ),
                    )
            })
            .into_any_element()
    }

    pub(super) fn settings_privacy_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let clear_all_pending = owner_state.clear_all_pending;
        let privacy_owner = self.owner.clone();

        settings_inline()
            .child(
                Button::new("settings-window-clear-all")
                    .danger()
                    .h(px(32.))
                    .icon(IconName::Delete)
                    .label(tr(language, "删除全部历史", "Delete all history"))
                    .disabled(clear_all_pending)
                    .on_click(move |_, window, cx| {
                        let dialog_owner = privacy_owner.clone();
                        let _ = dialog_owner.update(cx, |owner, cx| {
                            owner.clear_all_error = None;
                            cx.notify();
                        });
                        window.open_dialog(cx, move |dialog, _, cx| {
                            let (pending, error) = dialog_owner
                                .upgrade()
                                .map(|owner| {
                                    let owner = owner.read(cx);
                                    (owner.clear_all_pending, owner.clear_all_error.clone())
                                })
                                .unwrap_or((false, None));
                            let confirm_owner = dialog_owner.clone();
                            let keyboard_owner = dialog_owner.clone();
                            let cancel_owner = dialog_owner.clone();
                            dialog
                                .bg(cx.theme().popover)
                                .title(tr(language, "删除全部历史", "Delete all history"))
                                .close_button(false)
                                .overlay_closable(false)
                                .on_ok(move |_, window, cx| {
                                    let _ = keyboard_owner.update(cx, |owner, cx| {
                                        owner.clear_all_history(cx);
                                    });
                                    window.refresh();
                                    false
                                })
                                .on_cancel(move |_, _, cx| {
                                    cancel_owner
                                        .upgrade()
                                        .is_none_or(|owner| !owner.read(cx).clear_all_pending)
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(tr(
                                            language,
                                            "删除所有分组中的全部历史？置顶和收藏也会删除。",
                                            "Delete all history from every group? Pinned and favorite items will also be deleted.",
                                        ))
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(tr(
                                                    language,
                                                    "设置和自定义分组会保留；内容及受管媒体无法恢复，可先导出备份。",
                                                    "Settings and custom groups will be kept. Content and managed media cannot be recovered; export a backup first if needed.",
                                                )),
                                        )
                                        .when_some(error, |content, error| {
                                            content.child(Alert::error("clear-all-error", error).small())
                                        }),
                                )
                                .footer(
                                    DialogFooter::new()
                                        .child(
                                            Button::new("clear-all-history-cancel")
                                                .outline()
                                                .label(tr(language, "取消", "Cancel"))
                                                .disabled(pending)
                                                .on_click(|_, window, cx| window.close_dialog(cx)),
                                        )
                                        .child(
                                            Button::new("clear-all-history-confirm")
                                                .danger()
                                                .label(if pending {
                                                    tr(language, "正在删除…", "Deleting…")
                                                } else {
                                                    tr(language, "确认删除全部历史", "Delete all history")
                                                })
                                                .disabled(pending)
                                                .on_click(move |_, window, cx| {
                                                    let _ = confirm_owner.update(cx, |owner, cx| {
                                                        owner.clear_all_history(cx);
                                                    });
                                                    window.refresh();
                                                }),
                                        ),
                                )
                        });
                    }),
            )
            .into_any_element()
    }
}
