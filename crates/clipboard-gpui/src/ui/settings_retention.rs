use super::settings_common::{
    settings_column_field, settings_columns, settings_inline, settings_number_field, settings_stack,
};
use super::*;
use clipboard_core::{DeletionOutcome, retention::RetentionPolicy};
use gpui_kit::component::input::NumberInput;

impl ClipboardView {
    fn save_retention(&mut self, policy: RetentionPolicy, confirmed: bool, cx: &mut Context<Self>) {
        if !self.retention.pending
            && (!self.retention_saved || policy != self.retention.value)
            && self.send(Command::SetRetention { policy, confirmed }, cx)
        {
            self.retention.pending = true;
            cx.notify();
        }
    }

    pub(super) fn apply_retention_saved(
        &mut self,
        result: Result<(RetentionPolicy, DeletionOutcome), String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.retention.pending = false;
        match result {
            Ok((policy, outcome)) => {
                self.retention.value = policy;
                self.retention_saved = true;
                if let Some(warning) = outcome.cleanup_warning {
                    self.set_feedback_message(format!(
                        "{}: {warning}",
                        tr(
                            self.language,
                            "策略已保存，但附件回收不完整",
                            "Policy saved, but media cleanup is incomplete"
                        )
                    ));
                    self.is_error = true;
                } else {
                    self.is_error = false;
                }
                self.refresh_data_size(cx);
                self.refresh_daily_counts(cx);
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "保存保留策略失败",
                        "Unable to save retention policy"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }
}

impl SettingsWindowView {
    pub(super) fn settings_retention_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let saved = owner.read(cx).retention;
        let language = self.language;
        settings_stack()
            .child(
                settings_columns()
                    .child(
                        settings_column_field(tr(language, "保留天数", "Retention days")).child(
                            settings_number_field(
                                NumberInput::new(&self.retention_days_input),
                                saved.pending,
                            ),
                        ),
                    )
                    .child(
                        settings_column_field(tr(language, "每组数量", "Items per group")).child(
                            settings_number_field(
                                NumberInput::new(&self.retention_count_input),
                                saved.pending,
                            ),
                        ),
                    ),
            )
            .when_some(self.retention_error.clone(), |body, error| {
                body.child(Alert::error("retention-invalid", error).small())
            })
            .child(
                settings_inline().child(
                    Button::new("save-retention")
                        .primary()
                        .h(px(visual::CONTROL_HEIGHT_LARGE))
                        .label(tr(language, "保存保留策略", "Save retention policy"))
                        .disabled(saved.pending)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.request_retention_save(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn request_retention_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let days = self.retention_days_input.read(cx).value().parse::<u32>();
        let count = self.retention_count_input.read(cx).value().parse::<u32>();
        let (Ok(days), Ok(max_count)) = (days, count) else {
            self.retention_error = Some(
                tr(
                    self.language,
                    "请输入 0 到 4294967295 之间的整数",
                    "Enter whole numbers from 0 to 4294967295",
                )
                .into(),
            );
            cx.notify();
            return;
        };
        self.retention_error = None;
        let policy = RetentionPolicy { days, max_count };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let saved = owner.read(cx).retention;
        let explicitly_saved = owner.read(cx).retention_saved;
        if saved.pending || (explicitly_saved && policy == saved.value) {
            return;
        }
        if explicitly_saved && !policy.requires_confirmation(saved.value) {
            owner.update(cx, |owner, cx| owner.save_retention(policy, false, cx));
            return;
        }
        let owner = self.owner.clone();
        let language = self.language;
        window.open_dialog(cx, move |dialog, _, cx| {
            let owner = owner.clone();
            let submit_owner = owner.clone();
            dialog_shell(dialog, cx)
                .title(dialog_title(
                    tr(language, "保存保留策略？", "Save retention policy?"),
                    cx,
                ))
                .child(dialog_copy().child({
                    let days = if policy.days == 0 {
                        tr(language, "不限天数", "no age limit").to_owned()
                    } else if language == LanguagePreference::English {
                        format!("{} days", policy.days)
                    } else {
                        format!("{} 天", policy.days)
                    };
                    let count = if policy.max_count == 0 {
                        tr(language, "不限条数", "no item limit").to_owned()
                    } else if language == LanguagePreference::English {
                        format!("{} items per group", policy.max_count)
                    } else {
                        format!("每组 {} 条", policy.max_count)
                    };
                    if language == LanguagePreference::English {
                        format!(
                            "Apply {days} and {count} to every group, including existing custom groups? Unpinned, non-favorite items beyond either limit are deleted immediately and cannot be recovered. Export a backup first if needed."
                        )
                    } else {
                        format!(
                            "将「{days}、{count}」应用到全部分组（含现有自定义分组）？超出限制的未置顶、未收藏记录会立即删除且无法恢复。需要时请先导出备份。"
                        )
                    }
                }))
                .footer(dialog_footer(cx)
                    .child(Button::new("retention-cancel").outline()
                        .label(tr(language, "取消", "Cancel"))
                        .on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("retention-confirm").danger()
                        .label(tr(language, "保存并删除记录", "Save and delete items"))
                        .on_click(move |_, window, cx| {
                            let _ = submit_owner.update(cx, |owner, cx| owner.save_retention(policy, true, cx));
                            window.close_dialog(cx);
                        })))
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |owner, cx| owner.save_retention(policy, true, cx));
                    true
                })
        });
        cx.notify();
    }
}
