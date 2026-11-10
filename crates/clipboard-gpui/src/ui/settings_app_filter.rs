use super::settings_common::{
    SETTINGS_INSET_LIST_MAX_HEIGHT, choice_tabs, owner_state, pref_checkbox, settings_description,
    settings_field, settings_inline, settings_inset_list, settings_inset_list_row, settings_stack,
};
use super::*;
use gpui_kit::component::{accordion::Accordion, alert::Alert};

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
        let rule_count = app_filter.rules.len();
        let running_count = running_apps.len();

        settings_stack()
            .child(
                pref_checkbox(
                    cx,
                    "app-filter-enabled",
                    tr(language, "启用应用过滤", "Enable app filter"),
                    app_filter.enabled,
                    app_filter_pending,
                    &self.owner,
                    |owner| owner.app_filter.value.clone(),
                    |next, checked| next.enabled = checked,
                    ClipboardView::save_app_filter,
                )
                .w_full(),
            )
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
                        cx,
                    ),
                ),
            )
            .child(settings_description(
                if app_filter.mode == AppFilterMode::Blacklist {
                    tr(
                        language,
                        "匹配规则的应用不记录；来源未知时继续记录",
                        "Matching apps are skipped; unknown sources are recorded",
                    )
                } else {
                    tr(
                        language,
                        "只记录匹配规则的应用；规则为空或来源未知时继续记录",
                        "Only matching apps are recorded; empty rules or unknown sources are recorded",
                    )
                },
                cx,
            ))
            .child(
                settings_field(tr(language, "进程名或规则", "Process name or pattern"))
                    .child(
                        settings_inline()
                            .child(div().flex_1().min_w_0().child(
                                Input::new(&self.app_filter_input)
                                    .w_full()
                                    .refine_style(
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
            )
            .when(self.app_filter_error, |panel| {
                panel.child(
                    Alert::error(
                        "app-filter-error",
                        tr(
                            language,
                            "规则无效、数量已达上限或保存尚未完成",
                            "Invalid rule, rule limit reached, or save still in progress",
                        ),
                    )
                    .small(),
                )
            })
            .when(rule_count == 0, |panel| {
                panel.child(settings_description(
                    tr(
                        language,
                        "尚无规则，可输入进程名或 *、? 通配符",
                        "No rules yet. Enter a process name or use * and ? wildcards",
                    ),
                    cx,
                ))
            })
            .when(rule_count > 0, |panel| {
                panel.child(
                    settings_inset_list(cx).children(
                        app_filter
                            .rules
                            .iter()
                            .enumerate()
                            .map(|(index, rule)| {
                                app_filter_rule_row(
                                    language,
                                    rule,
                                    index,
                                    index + 1 == rule_count,
                                    app_filter_pending,
                                    &self.owner,
                                    cx,
                                )
                            }),
                    ),
                )
            })
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
                            settings_stack()
                                .when(self.app_picker_open, |content| {
                                    content
                                        .when(running_apps_pending, |content| {
                                            content.child(settings_description(
                                                tr(
                                                    language,
                                                    "正在读取运行中应用…",
                                                    "Loading running apps…",
                                                ),
                                                cx,
                                            ))
                                        })
                                        .when(
                                            !running_apps_pending && running_count == 0,
                                            |content| {
                                                content.child(settings_description(
                                                    tr(
                                                        language,
                                                        "没有找到可选应用",
                                                        "No running apps found",
                                                    ),
                                                    cx,
                                                ))
                                            },
                                        )
                                        .when(
                                            !running_apps_pending && running_count > 0,
                                            |content| {
                                                content.child(
                                                    settings_inset_list(cx)
                                                        .max_h(px(SETTINGS_INSET_LIST_MAX_HEIGHT))
                                                        .overflow_y_scrollbar()
                                                        .children(
                                                            running_apps
                                                                .iter()
                                                                .enumerate()
                                                                .map(|(index, app)| {
                                                                    running_app_row(
                                                                        language,
                                                                        app,
                                                                        index,
                                                                        index + 1
                                                                            == running_count,
                                                                        &app_filter.rules,
                                                                        app_filter_pending,
                                                                        &self.owner,
                                                                        cx,
                                                                    )
                                                                }),
                                                        ),
                                                )
                                            },
                                        )
                                }),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn app_filter_rule_row(
    language: LanguagePreference,
    rule: &str,
    index: usize,
    last: bool,
    pending: bool,
    owner: &WeakEntity<ClipboardView>,
    cx: &App,
) -> Div {
    let owner = owner.clone();
    let remove_rule = rule.to_owned();
    settings_inset_list_row(cx, last)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(visual::TEXT_BODY))
                .whitespace_normal()
                .child(rule.to_owned()),
        )
        .child(
            Button::new(format!("app-filter-remove-{index}"))
                .h(px(visual::CONTROL_HEIGHT))
                .flex_shrink_0()
                .ghost()
                .label(tr(language, "移除", "Remove"))
                .disabled(pending)
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |owner, cx| {
                        let next = owner.app_filter.value.clone().without_rule(&remove_rule);
                        owner.save_app_filter(next, cx);
                    });
                }),
        )
}

fn running_app_row(
    language: LanguagePreference,
    app: &RunningApp,
    index: usize,
    last: bool,
    rules: &[String],
    pending: bool,
    owner: &WeakEntity<ClipboardView>,
    cx: &App,
) -> Div {
    let owner = owner.clone();
    let process = app.process.clone();
    let already_added = rules.iter().any(|rule| rule.eq_ignore_ascii_case(&process));
    settings_inset_list_row(cx, last)
        .when_some(app.icon.clone(), |row, path| {
            row.child(
                div().flex_none().child(
                    img(std::path::PathBuf::from(path))
                        .w(px(visual::ICON_SIZE))
                        .h(px(visual::ICON_SIZE))
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(|| div().into_any_element()),
                ),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(visual::TEXT_BODY))
                .whitespace_normal()
                .child(app.process.clone()),
        )
        .child(
            div().flex_none().self_center().child(
                Button::new(format!("app-filter-pick-{index}"))
                    .h(px(visual::CONTROL_HEIGHT))
                    .outline()
                    .small()
                    .tooltip(app.process.clone())
                    .label(tr(language, "添加", "Add"))
                    .disabled(pending || already_added)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |owner, cx| {
                            if let Some(next) = owner.app_filter.value.clone().with_rule(&process) {
                                owner.save_app_filter(next, cx);
                            }
                        });
                    }),
            ),
        )
}
