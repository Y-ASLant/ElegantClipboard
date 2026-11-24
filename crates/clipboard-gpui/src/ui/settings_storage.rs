use super::settings_common::{owner_state, settings_description, settings_inline, settings_stack};
use super::*;
use gpui_kit::component::alert::Alert;

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
        let clear_all_pending = owner_state.clear_all_pending;
        let data_dir = clipboard_platform::display_path(&owner_state.service.data_dir);
        let path_prefix = tr(language, "保存位置：", "Save location: ");
        let relocate_owner = self.owner.clone();
        let path_busy = owner_state.data_import_phase.is_some()
            || clear_all_pending
            || export_pending
            || maintenance_pending;

        settings_stack()
            .child(
                settings_inline()
                    .items_center()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(visual::SPACE_1))
                            .child(
                                div()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(path_prefix),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .font_family("monospace")
                                    .text_color(cx.theme().muted_foreground)
                                    .child(data_dir),
                            ),
                    )
                    .child(
                        Button::new("settings-change-data-dir")
                            .outline()
                            .h(px(visual::CONTROL_HEIGHT))
                            .label(tr(language, "更改位置", "Change location"))
                            .disabled(path_busy)
                            .on_click(move |_, window, cx| {
                                ClipboardView::start_change_data_directory(
                                    relocate_owner.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    ),
            )
            .child(
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
                                    .accessibility_label(tr(
                                        language,
                                        "整理数据库",
                                        "Optimize database",
                                    ))
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
                                    .accessibility_label(tr(
                                        language,
                                        "打开数据目录",
                                        "Open data folder",
                                    ))
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
                                    .accessibility_label(tr(language, "导出备份", "Export backup"))
                                    .disabled(export_pending)
                                    .on_click(move |_, _, cx| {
                                        let _ = export_owner.update(cx, |owner, cx| {
                                            owner.start_export(cx);
                                        });
                                    }),
                            )
                            .child(self.clear_all_history_button(language, clear_all_pending)),
                    ),
            )
            .into_any_element()
    }

    /// 导入/恢复共用进度面：选路径与写入阶段同一套结构，文案按来源区分。
    fn render_data_import_progress(
        &self,
        language: LanguagePreference,
        phase: data_import::ImportPhase,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use data_import::{ImportPhase, ImportSource};
        let (title, detail) = match phase {
            ImportPhase::ChoosingPaths(ImportSource::LegacyBackup) => (
                tr(language, "选择旧版备份", "Choosing legacy backup"),
                tr(
                    language,
                    "选择 1.2.10 ZIP。确认后写入当前保存位置并重启（不含旧设置）。",
                    "Pick a 1.2.10 ZIP. It writes into the current save location and restarts (no legacy settings).",
                ),
            ),
            ImportPhase::ChoosingPaths(ImportSource::Backup) => (
                tr(language, "选择 GPUI 备份", "Choosing GPUI backup"),
                tr(
                    language,
                    "选择 GPUI 备份。确认后完整恢复到当前保存位置并重启。",
                    "Pick a GPUI backup. It fully restores into the current save location and restarts.",
                ),
            ),
            ImportPhase::Importing(ImportSource::LegacyBackup)
            | ImportPhase::Importing(ImportSource::Backup) => (
                tr(
                    language,
                    "正在准备导入并重启",
                    "Preparing import and restart",
                ),
                tr(
                    language,
                    "即将重启并写入当前保存位置。",
                    "The app will restart and write into the current save location.",
                ),
            ),
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_2))
            .p(px(visual::SPACE_3))
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(visual::SPACE_2))
                    .child(
                        gpui_kit::component::spinner::Spinner::new()
                            .with_size(gpui_kit::component::Size::Small)
                            .color(cx.theme().primary),
                    )
                    .child(div().text_size(px(14.)).font_semibold().child(title)),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(cx.theme().muted_foreground)
                    .whitespace_normal()
                    .child(detail),
            )
            .into_any_element()
    }

    pub(super) fn settings_import_content(&self, cx: &mut Context<Self>) -> AnyElement {
        use data_import::ImportSource;
        let Some(owner) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner.language;
        let phase = owner.data_import_phase;
        let reset_pending = owner.reset_settings_pending;
        let outcome_description = owner
            .data_import_result
            .as_ref()
            .filter(|_| phase.is_none())
            .map(|outcome| (outcome.is_incomplete(), outcome.description(language)));
        let choices = [
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
        let busy = phase.is_some() || reset_pending;
        let progress = phase.map(|phase| self.render_data_import_progress(language, phase, cx));
        settings_stack()
            .child(
                settings_inline()
                    .children(choices.into_iter().map(|(id, label, source)| {
                        let owner = self.owner.clone();
                        Button::new(id)
                            .outline()
                            .h(px(visual::CONTROL_HEIGHT))
                            .label(label)
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                ClipboardView::start_data_import(
                                    owner.clone(),
                                    source,
                                    window,
                                    cx,
                                );
                            })
                    }))
                    .child(self.reset_settings_button(language, reset_pending, phase.is_some())),
            )
            .when_some(progress, |content, progress| content.child(progress))
            .when_some(outcome_description, |content, (incomplete, mut description)| {
                let result = if incomplete {
                    description.push_str(tr(
                        language,
                        "。要找回缺失的附件，请将完整备份恢复到另一个空目录。",
                        ". To recover the missing attachments, restore a complete backup into another empty folder.",
                    ));
                    Alert::warning("restore-incomplete", description)
                        .title(tr(language, "恢复不完整", "Incomplete restore"))
                        .into_any_element()
                } else {
                    settings_description(description, cx).into_any_element()
                };
                content.child(result)
            })
            .into_any_element()
    }

    fn reset_settings_button(
        &self,
        language: LanguagePreference,
        reset_pending: bool,
        import_pending: bool,
    ) -> Button {
        let owner = self.owner.clone();
        Button::new("settings-reset-app")
            .outline()
            .h(px(visual::CONTROL_HEIGHT))
            .label(tr(language, "重置应用", "Reset app"))
            .tooltip(tr(
                language,
                "恢复默认设置与默认保存位置，保留剪贴板历史",
                "Restore default settings and save location; keep clipboard history",
            ))
            .disabled(reset_pending || import_pending)
            .on_click(move |_, window, cx| {
                open_settings_danger_dialog(
                    window,
                    cx,
                    owner.clone(),
                    SettingsDangerDialog {
                        title: tr(language, "重置应用设置？", "Reset app settings?"),
                        body: tr(
                            language,
                            "主题、热键、粘贴与采集等设置将恢复默认；自定义保存位置也会改回默认目录。",
                            "Theme, hotkeys, paste, and capture settings return to defaults. A custom save location moves back to the default folder.",
                        ),
                        detail: tr(
                            language,
                            "剪贴板历史、分组与图片会随数据一并保留并迁回默认位置。完成后应用会自动重启。",
                            "Clipboard history, groups, and images are kept and moved to the default location. The app restarts when done.",
                        ),
                        cancel_id: "reset-settings-cancel",
                        confirm_id: "reset-settings-confirm",
                        error_id: "reset-settings-error",
                        cancel_label: tr(language, "取消", "Cancel"),
                        confirm_label: tr(language, "重置并重启", "Reset and restart"),
                        pending_label: tr(language, "正在重置", "Resetting"),
                    },
                    |view| view.reset_settings_error = None,
                    |view| (view.reset_settings_pending, view.reset_settings_error.clone()),
                    |view, cx| view.confirm_reset_settings(cx),
                );
            })
    }

    fn clear_all_history_button(
        &self,
        language: LanguagePreference,
        clear_all_pending: bool,
    ) -> Button {
        let owner = self.owner.clone();
        Button::new("settings-window-clear-all")
            .danger()
            .h(px(visual::CONTROL_HEIGHT_LARGE))
            .icon(IconName::Delete)
            .label(tr(language, "删除全部历史", "Delete all history"))
            .tooltip(tr(
                language,
                "永久删除全部剪贴板历史",
                "Permanently delete all clipboard history",
            ))
            .disabled(clear_all_pending)
            .on_click(move |_, window, cx| {
                open_settings_danger_dialog(
                    window,
                    cx,
                    owner.clone(),
                    SettingsDangerDialog {
                        title: tr(language, "删除全部历史？", "Delete all history?"),
                        body: tr(
                            language,
                            "所有分组中的记录都会被删除，包括置顶和收藏。",
                            "All history in every group is deleted, including pinned and favorite items.",
                        ),
                        detail: tr(
                            language,
                            "设置和自定义分组会保留；内容及受管媒体无法恢复，可先导出备份。",
                            "Settings and custom groups will be kept. Content and managed media cannot be recovered; export a backup first if needed.",
                        ),
                        cancel_id: "clear-all-history-cancel",
                        confirm_id: "clear-all-history-confirm",
                        error_id: "clear-all-error",
                        cancel_label: tr(language, "取消", "Cancel"),
                        confirm_label: tr(language, "删除", "Delete"),
                        pending_label: tr(language, "正在删除", "Deleting"),
                    },
                    |view| view.clear_all_error = None,
                    |view| (view.clear_all_pending, view.clear_all_error.clone()),
                    |view, cx| view.clear_all_history(cx),
                );
            })
    }
}
