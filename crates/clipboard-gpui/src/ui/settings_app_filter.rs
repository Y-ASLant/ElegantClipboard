use super::settings_common::{
    choice_tabs, owner_state, settings_column, settings_columns, settings_description,
    settings_field, settings_inline, settings_stack,
};
use super::*;
use gpui_kit::component::{accordion::Accordion, alert::Alert, checkbox::Checkbox};

impl SettingsWindowView {
    pub(super) fn settings_app_filter_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        if let Some(reason) = crate::capabilities::source_app_unavailable_reason(language) {
            return Alert::warning("source-app-capability", reason)
                .small()
                .into_any_element();
        }
        let app_filter = owner_state.app_filter.value.clone();
        let app_filter_pending = owner_state.app_filter.pending;
        let running_apps = if self.app_picker_open {
            owner_state.running_apps.clone()
        } else {
            Vec::new()
        };
        let running_apps_pending = owner_state.running_apps_pending;
        let filter_enable_owner = self.owner.clone();

        settings_stack()
            .child(
                Checkbox::new("app-filter-enabled")
                    .w_full()
                    .min_h(px(32.))
                    .py(px(6.))
                    .text_size(px(14.))
                    .label(tr(language, "启用应用过滤", "Enable app filter"))
                    .checked(app_filter.enabled)
                    .disabled(app_filter_pending)
                    .on_change(move |checked, _, cx| {
                        let _ = filter_enable_owner.update(cx, |owner, cx| {
                            let mut next = owner.app_filter.value.clone();
                            next.enabled = *checked;
                            owner.save_app_filter(next, cx);
                        });
                    }),
            )
            .child(
                settings_columns()
                    .child(
                        settings_column()
                            .child(
                                settings_field(tr(language, "过滤模式", "Filter mode")).child(
                                    choice_tabs(
                                        "app-filter-mode",
                                        &self.owner,
                                        [AppFilterMode::Blacklist, AppFilterMode::Whitelist],
                                        [
                                            tr(language, "黑名单", "Blocklist"),
                                            tr(language, "白名单", "Allowlist"),
                                        ],
                                        app_filter.mode,
                                        app_filter_pending,
                                        |owner| owner.app_filter.value.mode,
                                        |owner, mode, cx| {
                                            let mut next = owner.app_filter.value.clone();
                                            next.mode = mode;
                                            owner.save_app_filter(next, cx);
                                        },
                                    ),
                                ),
                            )
                            .child(settings_description(
                                if app_filter.mode == AppFilterMode::Blacklist {
                                    tr(language, "匹配规则的应用不记录；来源未知时继续记录", "Matching apps are skipped; unknown sources are recorded")
                                } else {
                                    tr(language, "只记录匹配规则的应用；规则为空或来源未知时继续记录", "Only matching apps are recorded; empty rules or unknown sources are recorded")
                                },
                                cx,
                            )),
                    )
                    .child(
                        settings_column().child(
                            settings_field(tr(language, "进程名或规则", "Process name or pattern"))
                                .child(
                                    settings_inline()
                                        .child(div().flex_1().min_w_0().child(
                                            Input::new(&self.app_filter_input).refine_style(
                                                &StyleRefinement::default()
                                                    .h(px(visual::CONTROL_HEIGHT_LARGE)),
                                            ),
                                        ))
                                        .child(
                                            Button::new("app-filter-add-rule")
                                                .h(px(visual::CONTROL_HEIGHT_LARGE))
                                                .flex_shrink_0()
                                                .outline()
                                                .label(tr(language, "添加", "Add"))
                                                .disabled(app_filter_pending)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.add_app_filter_rule(window, cx);
                                                })),
                                        ),
                                ),
                        ),
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
                    settings_description(
                        tr(language, "尚无规则，可输入进程名或 *、? 通配符", "No rules yet. Enter a process name or use * and ? wildcards"),
                        cx,
                    ),
                )
            })
            .children(app_filter.rules.iter().enumerate().map(|(index, rule)| {
                let owner = self.owner.clone();
                let remove_rule = rule.clone();
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().flex_1().min_w_0().text_size(px(14.)).whitespace_normal().child(rule.clone()))
                    .child(
                        Button::new(format!("app-filter-remove-{index}"))
                            .h(px(32.))
                            .flex_shrink_0()
                            .ghost()
                            .label(tr(language, "移除", "Remove"))
                            .disabled(app_filter_pending)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |owner, cx| {
                                    let next = owner.app_filter.value.clone().without_rule(&remove_rule);
                                    owner.save_app_filter(next, cx);
                                });
                            }),
                    )
            }))
            .child(
                Accordion::new("app-filter-running-apps")
                    .w_full()
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
                                .w_full()
                                .min_w_0()
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
                                            .w_full()
                                            .min_w_0()
                                            .flex()
                                            .items_start()
                                            .gap_3()
                                            .py_2()
                                            .when_some(app.icon, |row, path| {
                                                row.child(img(std::path::PathBuf::from(path)).w(px(18.)).h(px(18.)).flex_shrink_0().object_fit(ObjectFit::Contain).with_fallback(|| div().into_any_element()))
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .min_w_0()
                                                            .whitespace_normal()
                                                            .child(app.process.clone()),
                                                    )
                                                    .child(settings_description(app.name, cx)),
                                            )
                                            .child(
                                                Button::new(format!("app-filter-pick-{index}"))
                                                    .h(px(32.))
                                                    .flex_shrink_0()
                                                    .ghost()
                                                    .tooltip(app.process)
                                                    .label(tr(language, "添加", "Add"))
                                                    .disabled(app_filter_pending || already_added)
                                                    .on_click(move |_, _, cx| {
                                                        let _ = owner.update(cx, |owner, cx| {
                                                            if let Some(next) = owner.app_filter.value.clone().with_rule(&process) {
                                                                owner.save_app_filter(next, cx);
                                                            }
                                                        });
                                                    }),
                                            )
                                    }))
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}
