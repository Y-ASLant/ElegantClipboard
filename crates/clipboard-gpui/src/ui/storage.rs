use super::*;
use directories::UserDirs;

impl ClipboardView {
    pub(super) fn refresh_data_size(&mut self, cx: &mut Context<Self>) {
        if self.data_size_pending {
            return;
        }
        if self.send(Command::QueryDataSize, cx) {
            self.data_size_pending = true;
            cx.notify();
        }
    }

    pub(super) fn refresh_daily_counts(&mut self, cx: &mut Context<Self>) {
        let start = chrono::Local::now().date_naive() - chrono::Duration::days(6);
        self.send(Command::QueryDailyCounts(start.to_string()), cx);
    }
    pub(super) fn start_export(&mut self, cx: &mut Context<Self>) {
        if self.export_pending {
            return;
        }
        let directory = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|path| path.to_path_buf()))
            .unwrap_or_else(|| self.service.data_dir.clone());
        self.export_pending = true;
        cx.notify();
        let selection = cx.prompt_for_new_path(&directory, Some("ElegantClipboard_backup.zip"));
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(path))) => {
                        if !this.send(Command::ExportBackup(path), cx) {
                            this.export_pending = false;
                        }
                    }
                    Ok(Ok(None)) => this.export_pending = false,
                    Ok(Err(error)) => {
                        this.export_pending = false;
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "无法选择备份路径",
                                "Unable to select a backup path"
                            )
                        ));
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.export_pending = false;
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "备份路径选择中断",
                                "Backup path selection was interrupted"
                            )
                        ));
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn start_save_as(
        &mut self,
        id: i64,
        suggested_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.save_as_pending.is_some() {
            return;
        }
        let directory = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|path| path.to_path_buf()))
            .unwrap_or_else(|| self.service.data_dir.clone());
        self.save_as_pending = Some(id);
        cx.notify();
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            let _ = view.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(destination))) => {
                        if !this.send(Command::SaveAs { id, destination }, cx) {
                            this.save_as_pending = None;
                        }
                    }
                    Ok(Ok(None)) => this.save_as_pending = None,
                    Ok(Err(error)) => {
                        this.save_as_pending = None;
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "无法选择另存为路径",
                                "Unable to select a destination"
                            )
                        ));
                        this.is_error = true;
                    }
                    Err(error) => {
                        this.save_as_pending = None;
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "另存为路径选择中断",
                                "Destination selection was interrupted"
                            )
                        ));
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn apply_saved_as(
        &mut self,
        id: i64,
        result: Result<std::path::PathBuf, String>,
    ) -> bool {
        if self.save_as_pending != Some(id) {
            return false;
        }
        self.save_as_pending = None;
        match result {
            Ok(destination) => {
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!("Saved to {}", destination.display())
                } else {
                    format!("已另存到 {}", destination.display())
                });
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "另存为失败", "Save as failed")
                ));
                self.is_error = true;
            }
        }
        true
    }
    pub(super) fn apply_data_size(&mut self, result: Result<DataSizeInfo, String>) -> bool {
        self.data_size_pending = false;
        match result {
            Ok(size) => {
                self.data_size = Some(size);
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "统计数据占用失败",
                        "Failed to calculate storage usage"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_daily_counts(
        &mut self,
        result: Result<Vec<(String, i64)>, String>,
    ) -> bool {
        match result {
            Ok(counts) => self.daily_counts = Some(counts),
            Err(error) => {
                self.daily_counts = None;
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "统计每日历史失败",
                        "Failed to count daily history"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }

    pub(super) fn apply_backup_exported(&mut self, result: Result<BackupReport, String>) -> bool {
        self.export_pending = false;
        match result {
            Ok(report) => {
                let included = report.included_images + report.included_icons;
                let missing = report.missing_images + report.missing_icons;
                self.set_feedback_message(if self.language == LanguagePreference::English {
                    format!(
                        "Backed up {} items and {} attachments to {}{}",
                        report.total_items,
                        included,
                        report.destination.display(),
                        if missing == 0 {
                            String::new()
                        } else {
                            format!("; {missing} source attachments were not included")
                        }
                    )
                } else {
                    format!(
                        "已备份 {} 条记录、{} 个附件到 {}{}",
                        report.total_items,
                        included,
                        report.destination.display(),
                        if missing == 0 {
                            String::new()
                        } else {
                            format!("；{missing} 个源附件未包含在备份中")
                        }
                    )
                });
                self.is_error = missing != 0;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "导出备份失败", "Failed to export backup")
                ));
                self.is_error = true;
            }
        }
        true
    }
}
