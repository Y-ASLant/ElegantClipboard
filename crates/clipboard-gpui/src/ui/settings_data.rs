use super::*;
use gpui_kit::component::chart::BarChart;
use gpui_kit::component::{alert::Alert, checkbox::Checkbox, dialog::DialogFooter};

impl SettingsWindowView {
    pub(super) fn settings_monitor_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let monitor_types = owner_state.monitor_types.value;
        let monitor_types_pending = owner_state.monitor_types.pending;

        type MonitorChoice<'a> = (
            &'a str,
            &'a str,
            bool,
            fn(&mut MonitorTypesPreference, bool),
        );
        let choices: [MonitorChoice<'_>; 6] = [
            (
                "monitor-text",
                tr(language, "文本", "Text"),
                monitor_types.text,
                |next, checked| next.text = checked,
            ),
            (
                "monitor-url",
                tr(language, "网址", "URL"),
                monitor_types.url,
                |next, checked| next.url = checked,
            ),
            (
                "monitor-html",
                "HTML",
                monitor_types.html,
                |next, checked| next.html = checked,
            ),
            ("monitor-rtf", "RTF", monitor_types.rtf, |next, checked| {
                next.rtf = checked
            }),
            (
                "monitor-image",
                tr(language, "图片", "Images"),
                monitor_types.image,
                |next, checked| next.image = checked,
            ),
            (
                "monitor-files",
                tr(language, "文件", "Files"),
                monitor_types.files,
                |next, checked| next.files = checked,
            ),
        ];
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(
                choices
                    .into_iter()
                    .map(|(id, label, selected, set_checked)| {
                        let owner = self.owner.clone();
                        let mut next = monitor_types;
                        set_checked(&mut next, !selected);
                        Checkbox::new(id)
                            .text_sm()
                            .label(label)
                            .checked(selected)
                            .disabled(monitor_types_pending || !next.valid())
                            .on_change(move |checked, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let mut next = owner.monitor_types.value;
                                    set_checked(&mut next, *checked);
                                    owner.save_monitor_types(next, cx);
                                });
                            })
                    }),
            )
            .into_any_element()
    }

    pub(super) fn settings_daily_counts_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner = owner_entity.read(cx);
        let Some(counts) = &owner.daily_counts else {
            return div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(tr(
                    owner.language,
                    "尚未获取每日统计",
                    "Daily counts are not available",
                ))
                .into_any_element();
        };
        let today = chrono::Local::now().date_naive();
        let data = (0..7)
            .rev()
            .map(|offset| {
                let day = (today - chrono::Duration::days(offset)).to_string();
                let count = counts
                    .iter()
                    .find(|(date, _)| date == &day)
                    .map_or(0, |(_, count)| *count);
                (day, count)
            })
            .collect::<Vec<_>>();
        let empty = data.iter().all(|(_, count)| *count == 0);
        div()
            .relative()
            .w_full()
            .min_w_0()
            .h(px(152.))
            .child(
                BarChart::new(data)
                    .id("daily-clipboard-counts")
                    .name(tr(owner.language, "历史条数", "Items"))
                    .band(|(date, _)| date[5..].to_string())
                    .value(|(_, count)| *count as f64)
                    .label(|(_, count)| count.to_string())
                    .grid(false),
            )
            .when(empty, |panel| {
                panel.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .w_full()
                        .text_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(
                            owner.language,
                            "最近 7 天没有历史记录",
                            "No items in the past 7 days",
                        )),
                )
            })
            .into_any_element()
    }

    pub(super) fn settings_storage_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
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
                if language == LanguagePreference::English {
                    format!(
                        "Total {} · Database {} · Images {} / {} · Staged {} / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes),
                        size.staged_count,
                        format_bytes(size.staged_bytes)
                    )
                } else {
                    format!(
                        "共 {} · 数据库 {} · 图片 {} 个 / {} · 暂存 {} 个 / {}",
                        format_bytes(size.total_bytes),
                        format_bytes(size.database_bytes),
                        size.image_count,
                        format_bytes(size.image_bytes),
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

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(data_size_detail),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .gap_2()
                    .ml_auto()
                    .child(
                        Button::new("settings-window-refresh")
                            .outline()
                            .small()
                            .icon(IconName::RotateCw)
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
                            .small()
                            .icon(IconName::HardDrive)
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
                            .small()
                            .icon(IconName::FolderOpen)
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
                            .small()
                            .icon(IconName::ExternalLink)
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

    pub(super) fn settings_privacy_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let clear_all_pending = owner_state.clear_all_pending;
        let privacy_owner = self.owner.clone();

        div()
            .child(
                Button::new("settings-window-clear-all")
                    .danger()
                    .small()
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
