use super::*;
use gpui_kit::component::{
    accordion::Accordion, alert::Alert, checkbox::Checkbox, dialog::DialogFooter,
};

impl SettingsWindowView {
    pub(super) fn settings_monitor_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let monitor_types = owner_state.monitor_types;
        let monitor_types_pending = owner_state.monitor_types_pending;

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
                                    let mut next = owner.monitor_types;
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
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(data_size_detail),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
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

    pub(super) fn settings_app_filter_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner_state = owner_entity.read(cx);
        let language = owner_state.language;
        let app_filter = owner_state.app_filter.clone();
        let app_filter_pending = owner_state.app_filter_pending;
        let running_apps = owner_state.running_apps.clone();
        let running_apps_pending = owner_state.running_apps_pending;
        let filter_enable_owner = self.owner.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Checkbox::new("app-filter-enabled")
                    .text_sm()
                    .label(tr(language, "启用应用过滤", "Enable app filter"))
                    .checked(app_filter.enabled)
                    .disabled(app_filter_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = filter_enable_owner.update(cx, |owner, cx| {
                            let mut next = owner.app_filter.clone();
                            next.enabled = *checked;
                            owner.save_app_filter(next, cx);
                        });
                    }),
            )
            .child(div().flex().flex_wrap().gap_2().children(
                [
                    (AppFilterMode::Blacklist, "app-filter-blacklist", tr(language, "黑名单", "Blocklist")),
                    (AppFilterMode::Whitelist, "app-filter-whitelist", tr(language, "白名单", "Allowlist")),
                ]
                .into_iter()
                .map(|(mode, id, label)| {
                    let owner = self.owner.clone();
                    Button::new(id)
                        .small()
                        .outline()
                        .label(label)
                        .selected(app_filter.mode == mode)
                        .disabled(app_filter_pending)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |owner, cx| {
                                let mut next = owner.app_filter.clone();
                                next.mode = mode;
                                owner.save_app_filter(next, cx);
                            });
                        })
                }),
            ))
            .child(
                div().text_xs().text_color(cx.theme().muted_foreground).child(
                    if app_filter.mode == AppFilterMode::Blacklist {
                        tr(language, "匹配规则的应用不记录；来源未知时继续记录", "Matching apps are skipped; unknown sources are recorded")
                    } else {
                        tr(language, "只记录匹配规则的应用；规则为空或来源未知时继续记录", "Only matching apps are recorded; empty rules or unknown sources are recorded")
                    },
                ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.app_filter_input)))
                    .child(
                        Button::new("app-filter-add-rule")
                            .small()
                            .outline()
                            .label(tr(language, "添加", "Add"))
                            .disabled(app_filter_pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_app_filter_rule(window, cx);
                            })),
                    ),
            )
            .when(self.app_filter_error, |panel| {
                panel.child(
                    Alert::error(
                        "app-filter-error",
                        tr(language, "规则无效、数量已达上限或保存尚未完成", "Invalid rule, rule limit reached, or save still in progress"),
                    )
                    .small(),
                )
            })
            .when(app_filter.rules.is_empty(), |panel| {
                panel.child(
                    div().text_xs().text_color(cx.theme().muted_foreground).child(
                        tr(language, "尚无规则，可输入进程名或 *、? 通配符", "No rules yet. Enter a process name or use * and ? wildcards"),
                    ),
                )
            })
            .children(app_filter.rules.iter().enumerate().map(|(index, rule)| {
                let owner = self.owner.clone();
                let remove_rule = rule.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().text_sm().text_ellipsis().child(rule.clone()))
                    .child(
                        Button::new(format!("app-filter-remove-{index}"))
                            .small()
                            .ghost()
                            .label(tr(language, "移除", "Remove"))
                            .disabled(app_filter_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let next = owner.app_filter.clone().without_rule(&remove_rule);
                                    owner.save_app_filter(next, cx);
                                });
                            }),
                    )
            }))
            .child(
                Accordion::new("app-filter-running-apps")
                    .small()
                    .on_toggle_click({
                        let settings = cx.entity().downgrade();
                        move |indices, _, cx| {
                            let _ = settings.update(cx, |this, cx| {
                                let open = indices.contains(&0);
                                if this.app_picker_open != open {
                                    this.app_picker_open = open;
                                    if open && let Some(owner) = this.owner.upgrade() {
                                        owner.update(cx, |owner, cx| owner.load_running_apps(cx));
                                    }
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .item(|item| {
                        item.title(if self.app_picker_open {
                            tr(language, "收起运行中应用", "Hide running apps")
                        } else {
                            tr(language, "选择运行中应用", "Choose a running app")
                        })
                        .open(self.app_picker_open)
                        .child(
                            div()
                                .max_h(px(240.))
                                .overflow_y_scrollbar()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .when(self.app_picker_open, |list| {
                                    list.when(running_apps_pending, |list| {
                                        list.child(tr(language, "正在读取运行中应用…", "Loading running apps…"))
                                    })
                                    .when(!running_apps_pending && running_apps.is_empty(), |list| {
                                        list.child(tr(language, "没有找到可选应用", "No running apps found"))
                                    })
                                    .children(running_apps.into_iter().enumerate().map(|(index, app)| {
                                        let owner = self.owner.clone();
                                        let process = app.process.clone();
                                        let already_added = app_filter.rules.iter().any(|rule| rule.eq_ignore_ascii_case(&process));
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .when_some(app.icon, |row, path| {
                                                row.child(img(std::path::PathBuf::from(path)).w(px(18.)).h(px(18.)).object_fit(ObjectFit::Contain).with_fallback(|| div().into_any_element()))
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        Button::new(format!("app-filter-pick-{index}"))
                                                            .small()
                                                            .ghost()
                                                            .label(app.process)
                                                            .disabled(app_filter_pending || already_added)
                                                            .on_click(move |_, _, cx| {
                                                                let _ = owner.update(cx, |owner, cx| {
                                                                    if let Some(next) = owner.app_filter.clone().with_rule(&process) {
                                                                        owner.save_app_filter(next, cx);
                                                                    }
                                                                });
                                                            }),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .text_ellipsis()
                                                            .child(app.name),
                                                    ),
                                            )
                                    }))
                                }),
                        )
                    }),
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
                                .on_ok(move |_, _, cx| {
                                    let _ = keyboard_owner.update(cx, |owner, cx| {
                                        owner.clear_all_history(cx);
                                    });
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
                                                .on_click(move |_, _, cx| {
                                                    let _ = confirm_owner.update(cx, |owner, cx| {
                                                        owner.clear_all_history(cx);
                                                    });
                                                }),
                                        ),
                                )
                        });
                    }),
            )
            .into_any_element()
    }

    pub(super) fn settings_about_details_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(
                [
                    (
                        "about-author",
                        tr(language, "作者", "Author"),
                        "ASLant",
                        ProjectLink::Author,
                    ),
                    (
                        "about-repository",
                        "GitHub",
                        "ElegantClipboard",
                        ProjectLink::Repository,
                    ),
                    (
                        "about-issues",
                        tr(language, "反馈", "Feedback"),
                        tr(language, "提交问题", "Submit issue"),
                        ProjectLink::Issues,
                    ),
                ]
                .map(|(id, caption, label, link)| {
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(caption),
                        )
                        .child(
                            Button::new(id)
                                .small()
                                .ghost()
                                .icon(IconName::ExternalLink)
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.link_error =
                                        links::open(link).err().map(|error| error.to_string());
                                    cx.notify();
                                })),
                        )
                }),
            )
            .when_some(self.link_error.clone(), |panel, error| {
                panel.child(Alert::error("about-link-error", error).small())
            })
            .into_any_element()
    }

    pub(super) fn settings_about_intro_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_entity) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let language = owner_entity.read(cx).language;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(format!("v{}", env!("CARGO_PKG_VERSION")))
            .child(tr(
                language,
                "数据保存在本机；可在“数据”中导出备份。",
                "Data stays on this device; export a backup from Data.",
            ))
            .into_any_element()
    }
}
