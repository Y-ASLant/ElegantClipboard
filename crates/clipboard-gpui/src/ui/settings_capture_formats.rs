use super::settings_common::{
    help_tooltip, owner_state, pref_switch, settings_description, settings_inline, settings_stack,
};
use super::*;
use gpui_kit::component::{accordion::Accordion, alert::Alert};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureFormatInputError {
    Invalid,
    Limit,
    Busy,
}

impl SettingsWindowView {
    pub(super) fn settings_capture_formats_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner_state) = owner_state(&self.owner, cx) else {
            return div().into_any_element();
        };
        let language = owner_state.language;
        let formats = owner_state.capture_formats.value.clone();
        let pending = owner_state.capture_formats.pending;
        let expanded = self.capture_formats_expanded;
        let error = self.capture_format_error;
        let count = formats.names.len();
        let title = match language {
            LanguagePreference::Chinese => format!("读取格式名单（{count}）"),
            LanguagePreference::English => format!("Read format list ({count})"),
        };
        let editor = format_editor(self, language, pending, cx);
        let names = formats.names.clone();
        let owner = self.owner.clone();

        settings_stack()
            .child(all_formats_switch(
                language,
                formats.all,
                pending,
                &self.owner,
                cx,
            ))
            .when(formats.all, |panel| {
                panel.child(settings_description(
                    tr(
                        language,
                        "读取全部格式时不会使用此名单；关闭该选项后，已保存的名单会继续生效。",
                        "This list is not used while Read all formats is on. Turn it off to use the saved list again.",
                    ),
                    cx,
                ))
            })
            .when(names.is_empty(), |panel| {
                panel.child(settings_description(
                    tr(
                        language,
                        "名单为空。关闭“读取全部格式”后，不会采集新的剪贴板内容。",
                        "The list is empty. New clipboard content is not captured while Read all formats is off.",
                    ),
                    cx,
                ))
            })
            .child(
                Accordion::new("capture-formats")
                    .w_full()
                    .on_toggle_click({
                        let settings = cx.entity().downgrade();
                        move |indices, _, cx| {
                            let _ = settings.update(cx, |this, cx| {
                                let open = indices.contains(&0);
                                if this.capture_formats_expanded != open {
                                    this.capture_formats_expanded = open;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .item(move |item| {
                        item.title(title).open(expanded).child(
                            settings_stack()
                                .gap_2()
                                .child(settings_description(
                                    tr(
                                        language,
                                        "填写剪贴板实际名称，例如 CF_UNICODETEXT、HTML Format、PNG 或 Biff12，不要填写“图片”或“文本”。",
                                        "Enter the exact clipboard name, such as CF_UNICODETEXT, HTML Format, PNG, or Biff12. Do not enter categories such as image or text.",
                                    ),
                                    cx,
                                ))
                                .when(error.is_some(), |panel| {
                                    panel.child(
                                        Alert::error(
                                            "capture-format-error",
                                            error_text(language, error),
                                        )
                                        .small(),
                                    )
                                })
                                .children(names.iter().map(|name| {
                                    format_name_row(language, name, pending, &owner, cx)
                                }))
                                .child(editor),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn add_capture_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.capture_format_input.read(cx).value();
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let outcome = owner.update(cx, |owner, cx| {
            if owner.capture_formats.pending {
                return AddOutcome::Busy;
            }
            let Some(next) = owner.capture_formats.value.clone().with_name(&name) else {
                return if CaptureFormatPreference::valid_name(&name) {
                    AddOutcome::Limit
                } else {
                    AddOutcome::Invalid
                };
            };
            if next == owner.capture_formats.value {
                return AddOutcome::Accepted;
            }
            owner.save_capture_formats(next, cx);
            if owner.capture_formats.pending {
                AddOutcome::Accepted
            } else {
                AddOutcome::Busy
            }
        });
        self.capture_format_error = match outcome {
            AddOutcome::Accepted => None,
            AddOutcome::Invalid => Some(CaptureFormatInputError::Invalid),
            AddOutcome::Limit => Some(CaptureFormatInputError::Limit),
            AddOutcome::Busy => Some(CaptureFormatInputError::Busy),
        };
        if outcome == AddOutcome::Accepted {
            self.capture_format_input
                .update(cx, |input, cx| input.set_value(String::new(), window, cx));
        }
        cx.notify();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AddOutcome {
    Accepted,
    Invalid,
    Limit,
    Busy,
}

fn error_text(
    language: LanguagePreference,
    error: Option<CaptureFormatInputError>,
) -> &'static str {
    match error {
        Some(CaptureFormatInputError::Limit) => tr(
            language,
            "名单已满。请先移除一个格式再添加。",
            "The list is full. Remove a format before adding another.",
        ),
        Some(CaptureFormatInputError::Busy) => tr(
            language,
            "尚未保存，请重试。",
            "Couldn’t save yet. Try again.",
        ),
        _ => tr(
            language,
            "请填写剪贴板格式名，不要填写“图片”或“文本”。",
            "Enter a clipboard format name, not a category.",
        ),
    }
}

fn all_formats_switch(
    language: LanguagePreference,
    all: bool,
    pending: bool,
    owner: &WeakEntity<ClipboardView>,
    cx: &mut Context<SettingsWindowView>,
) -> Div {
    settings_inline()
        .child(pref_switch(
            cx,
            "capture-all-formats",
            tr(language, "读取全部格式", "Read all formats"),
            all,
            pending,
            owner,
            |owner| owner.capture_formats.value.clone(),
            |next, checked| next.all = checked,
            ClipboardView::save_capture_formats,
        ))
        .child(help_tooltip(
            "capture-all-formats-help",
            tr(
                language,
                "读取来源应用提供的全部格式，兼容性更好；复制大内容时可能让来源应用变慢。",
                "Reads every format the source app offers. More compatible, but large copies can briefly slow that app.",
            ),
        ))
}

fn format_name_row(
    language: LanguagePreference,
    name: &str,
    pending: bool,
    owner: &WeakEntity<ClipboardView>,
    cx: &App,
) -> Div {
    let owner = owner.clone();
    let remove_name = name.to_owned();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap_3()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(visual::TEXT_BODY))
                .whitespace_normal()
                .child(name.to_owned()),
        )
        .child(
            Button::new(format!("capture-format-remove-{name}"))
                .h(px(visual::CONTROL_HEIGHT))
                .flex_shrink_0()
                .ghost()
                .label(tr(language, "移除", "Remove"))
                .disabled(pending)
                .on_click(move |_, _, cx| {
                    let remove_name = remove_name.clone();
                    let _ = owner.update(cx, |owner, cx| {
                        owner.save_capture_formats(
                            owner
                                .capture_formats
                                .value
                                .clone()
                                .without_name(&remove_name),
                            cx,
                        );
                    });
                }),
        )
}

fn format_editor(
    view: &SettingsWindowView,
    language: LanguagePreference,
    pending: bool,
    cx: &mut Context<SettingsWindowView>,
) -> Div {
    let owner = view.owner.clone();
    settings_inline()
        .child(
            div().flex_1().min_w_0().child(
                Input::new(&view.capture_format_input)
                    .w_full()
                    .refine_style(&StyleRefinement::default().h(px(visual::CONTROL_HEIGHT_LARGE))),
            ),
        )
        .child(
            Button::new("capture-format-add")
                .h(px(visual::CONTROL_HEIGHT_LARGE))
                .flex_shrink_0()
                .outline()
                .label(tr(language, "添加", "Add"))
                .disabled(pending)
                .on_click(cx.listener(|this, _, window, cx| this.add_capture_format(window, cx))),
        )
        .child(
            Button::new("capture-formats-restore")
                .h(px(visual::CONTROL_HEIGHT_LARGE))
                .flex_shrink_0()
                .ghost()
                .label(tr(language, "恢复默认", "Restore defaults"))
                .disabled(pending)
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |owner, cx| {
                        owner.save_capture_formats(
                            owner.capture_formats.value.clone().with_default_names(),
                            cx,
                        );
                    });
                }),
        )
}
