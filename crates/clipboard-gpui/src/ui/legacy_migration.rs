use super::*;
use clipboard_platform::{LegacyMigrationReport, LegacyMigrationStatus};
use gpui_kit::component::alert::Alert;

impl ClipboardView {
    pub(super) fn legacy_migration_visible(&self) -> bool {
        self.legacy_migration.is_some()
    }

    pub(super) fn start_legacy_migration(&mut self, cx: &mut Context<Self>) {
        if self.legacy_migration_pending || !self.legacy_migration_visible() {
            return;
        }
        if self.send(Command::CompleteLegacyMigration, cx) {
            self.legacy_migration_pending = true;
            self.legacy_migration_error = None;
            cx.notify();
        }
    }

    pub(super) fn apply_legacy_migration_completed(
        &mut self,
        result: Result<LegacyMigrationReport, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.legacy_migration_pending = false;
        match result {
            Ok(report) => {
                self.legacy_migration = None;
                self.legacy_migration_error = None;
                let message = if self.language == LanguagePreference::English {
                    format!(
                        "Upgrade cleanup finished: removed {} legacy file copies ({}); cleaned {} file references.",
                        report.staged_files_removed,
                        format_bytes(report.staged_bytes_removed),
                        report.payloads_cleaned
                    )
                } else {
                    format!(
                        "升级清理完成：已删除 {} 个旧文件副本（{}），清理 {} 条文件引用。",
                        report.staged_files_removed,
                        format_bytes(report.staged_bytes_removed),
                        report.payloads_cleaned
                    )
                };
                self.set_feedback_message(message);
                self.is_error = false;
                if self.search_auto_focus.value {
                    self.search.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.list_focus, cx);
                }
                true
            }
            Err(error) => {
                self.legacy_migration_error = Some(error.clone());
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "升级清理失败", "Upgrade cleanup failed")
                ));
                self.is_error = true;
                true
            }
        }
    }

    pub(super) fn render_legacy_migration(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language;
        let status = self.legacy_migration.unwrap_or(LegacyMigrationStatus {
            pending: true,
            staged_bytes: 0,
            staged_count: 0,
            residual_payloads: 0,
        });
        let pending = self.legacy_migration_pending;
        let title = tr(
            language,
            "升级到 2.0：清理旧版数据",
            "Upgrade to 2.0: clean legacy data",
        );
        let detail = tr(
            language,
            "检测到 1.2.10 升级痕迹或旧文件副本。2.0 只保留原始文件路径，不再保存 staged 副本。确认后将清理无用数据并完成迁移。",
            "1.2.10 upgrade residue or legacy file copies were found. 2.0 keeps original paths only and no longer stores staged copies. Confirm to clean unused data and finish migration.",
        );
        let stats = if language == LanguagePreference::English {
            format!(
                "Legacy copies: {} ({})\nPayload references still listing staged paths: {}",
                status.staged_count,
                format_bytes(status.staged_bytes),
                status.residual_payloads
            )
        } else {
            format!(
                "旧文件副本：{} 个（{}）\n仍含 staged 引用的记录：{} 条",
                status.staged_count,
                format_bytes(status.staged_bytes),
                status.residual_payloads
            )
        };
        let button_label = if pending {
            tr(language, "正在清理", "Cleaning")
        } else {
            tr(
                language,
                "开始清理并完成迁移",
                "Clean up and finish migration",
            )
        };

        div()
            .key_context("ClipboardApp")
            .track_focus(&self.list_focus)
            .size_full()
            .max_w(px(visual::CHOICE_MAX_WIDTH + 2. * visual::SPACE_4))
            .mx_auto()
            .px(px(visual::SPACE_4))
            .pb(px(visual::SPACE_4))
            .flex()
            .flex_col()
            .gap(px(visual::SPACE_3))
            .on_action(cx.listener(|this, _: &DismissOrHide, window, cx| {
                // 迁移未完成时不允许直接关掉界面。
                if this.legacy_migration_visible() {
                    this.show_window(window, cx);
                } else {
                    this.dismiss_or_hide(window, cx);
                }
            }))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(visual::SPACE_2))
                    .pb(px(visual::SPACE_2))
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_semibold()
                            .line_height(px(30.))
                            .child(title),
                    )
                    .child(settings_common::settings_description(detail, cx)),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .pr(px(visual::SPACE_4))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(visual::SPACE_3))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(cx.theme().muted_foreground)
                                    .whitespace_normal()
                                    .child(stats),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tr(
                                        language,
                                        "历史记录、图片与设置会保留。清理只删除数据目录内的 staged 副本及其数据库引用。",
                                        "History, images, and settings are kept. Cleanup only removes staged copies under the data directory and their database references.",
                                    )),
                            ),
                    ),
            )
            .when_some(self.legacy_migration_error.clone(), |view, error| {
                view.child(Alert::error("legacy-migration-error", error).small())
            })
            .child(
                div()
                    .flex_none()
                    .flex()
                    .justify_end()
                    .child(
                        Button::new("legacy-migration-confirm")
                            .primary()
                            .h(px(visual::CONTROL_HEIGHT_LARGE))
                            .label(button_label)
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_legacy_migration(cx);
                            })),
                    ),
            )
    }
}
