use super::settings_common::{
    settings_column, settings_columns, settings_description, settings_field, settings_inline,
    settings_stack,
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
                    self.set_feedback_message(if self.language == LanguagePreference::English {
                        format!("Retention saved; removed {} items", outcome.deleted)
                    } else {
                        format!("保留策略已保存，清理 {} 条记录", outcome.deleted)
                    });
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
        let explicitly_saved = owner.read(cx).retention_saved;
        let language = self.language;
        settings_stack()
            .child(settings_description(tr(
                language,
                "每个分组独立限制，置顶和收藏不计入数量且不会自动删除。0 表示关闭对应限制。",
                "Limits apply per group. Pinned and favorite items are excluded and protected. 0 disables a limit.",
            ), cx))
            .child(
                settings_columns()
                    .child(settings_column().child(
                        settings_field(tr(language, "保留天数", "Retention days")).child(
                            div().w(px(180.)).child(
                                NumberInput::new(&self.retention_days_input)
                                    .h(px(36.))
                                    .disabled(saved.pending),
                            ),
                        ),
                    ))
                    .child(settings_column().child(
                        settings_field(tr(language, "每组数量", "Items per group")).child(
                            div().w(px(180.)).child(
                                NumberInput::new(&self.retention_count_input)
                                    .h(px(36.))
                                    .disabled(saved.pending),
                            ),
                        ),
                    )),
            )
            .child(settings_description(
                if !explicitly_saved {
                    tr(language,
                        "尚未应用到全部分组：仅新采集时默认组保留 10000 条，不按天清理。首次保存需要确认。",
                        "Not applied to all groups: only new captures limit the default group to 10000 items, with no age limit. The first save requires confirmation.").to_owned()
                } else if language == LanguagePreference::English {
                    format!("Saved: {} days, {} items per group", saved.value.days, saved.value.max_count)
                } else {
                    format!("当前生效：{} 天，每组 {} 条", saved.value.days, saved.value.max_count)
                },
                cx,
            ))
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
            dialog.title(tr(language, "确认保留策略", "Confirm retention policy"))
                .bg(cx.theme().popover)
                .overlay_closable(false)
                .child(if language == LanguagePreference::English {
                    format!("Apply {} days and {} items per group to all groups, including existing custom groups (0 disables the limit)? Existing unpinned, non-favorite history beyond either limit will be deleted immediately. This cannot be undone; export a backup first if needed.", policy.days, policy.max_count)
                } else {
                    format!("向全部分组（包括现有自定义分组）应用 {} 天、每组 {} 条（0 表示关闭）？超出任一限制的现有未置顶且未收藏记录将立即删除，无法撤销；如有需要请先导出备份。", policy.days, policy.max_count)
                })
                .footer(DialogFooter::new()
                    .child(Button::new("retention-cancel").outline()
                        .label(tr(language, "取消", "Cancel"))
                        .on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("retention-confirm").danger()
                        .label(tr(language, "确认保存并清理", "Save and remove items"))
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
