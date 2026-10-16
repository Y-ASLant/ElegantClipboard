use super::*;
use anyhow::{Context as _, bail};

#[derive(Clone, Copy)]
pub(super) enum ImportSource {
    LegacyDatabase,
    LegacyBackup,
    Backup,
}

pub(super) struct ImportOutcome {
    pub directory: PathBuf,
    pub items: i64,
    pub attachments: Option<usize>,
    pub missing: usize,
}

impl ImportOutcome {
    pub(super) fn description(&self, language: LanguagePreference) -> String {
        let mut description = if language == LanguagePreference::English {
            format!(
                "Restored {} items to {}",
                self.items,
                self.directory.display()
            )
        } else {
            format!(
                "已恢复 {} 条记录到 {}",
                self.items,
                self.directory.display()
            )
        };
        match self.attachments {
            Some(count) => description.push_str(&if language == LanguagePreference::English {
                format!("; {count} attachments restored; {} attachments missing", self.missing)
            } else {
                format!("；已恢复 {count} 个附件，缺失 {} 个附件", self.missing)
            }),
            None => description.push_str(tr(
                language,
                "；仅复制数据库，附件仍依赖原路径，请保留旧版媒体目录",
                "; database only: attachments still use their original paths; keep the old media folders",
            )),
        }
        description
    }
}

impl ClipboardView {
    pub(super) fn start_data_import(&mut self, source: ImportSource, cx: &mut Context<Self>) {
        if self.data_import_pending {
            return;
        }
        self.data_import_pending = true;
        let language = self.language;
        let current_directory = self.service.data_dir.clone();
        let prompt = match source {
            ImportSource::LegacyDatabase => {
                tr(language, "选择旧版数据库", "Select legacy database")
            }
            ImportSource::LegacyBackup => {
                tr(language, "选择旧版 ZIP 备份", "Select legacy ZIP backup")
            }
            ImportSource::Backup => tr(language, "选择 GPUI 备份", "Select GPUI backup"),
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(prompt.into()),
        });
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result: anyhow::Result<Option<ImportOutcome>> = async {
                let Some(path) = selection.await??.and_then(|paths| paths.into_iter().next())
                else {
                    return Ok(None);
                };
                let destination = view.update(cx, |_, cx| {
                    cx.prompt_for_paths(PathPromptOptions {
                        files: false,
                        directories: true,
                        multiple: false,
                        prompt: Some(
                            tr(
                                language,
                                "选择空目标目录",
                                "Select empty destination folder",
                            )
                            .into(),
                        ),
                    })
                })?;
                let Some(directory) = destination
                    .await??
                    .and_then(|paths| paths.into_iter().next())
                else {
                    return Ok(None);
                };
                cx.background_executor()
                    .spawn(async move {
                        import_to_directory(source, &path, directory, &current_directory, language)
                    })
                    .await
                    .map(Some)
            }
            .await;
            let _ = view.update(cx, |this, cx| {
                this.data_import_pending = false;
                match result {
                    Ok(Some(outcome)) => {
                        this.set_feedback_message(outcome.description(this.language));
                        this.is_error = outcome.missing != 0;
                        this.data_import_result = Some(outcome);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        this.set_feedback_message(format!(
                            "{}: {error:#}",
                            tr(this.language, "导入 / 恢复失败", "Import / restore failed")
                        ));
                        this.is_error = true;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_imported_data(&mut self, cx: &mut Context<Self>) {
        let Some(outcome) = &self.data_import_result else {
            return;
        };
        let result = std::env::current_exe().and_then(|executable| {
            let mut command = std::process::Command::new(executable);
            command.arg("--data-dir").arg(&outcome.directory);
            if !self.monitoring {
                command.arg("--no-monitor");
            }
            command.spawn()
        });
        match result {
            Ok(_) => {
                self.set_feedback_message(
                    tr(
                        self.language,
                        "已请求打开恢复的数据",
                        "Requested to open restored data",
                    )
                    .into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "无法打开恢复的数据",
                        "Unable to open restored data"
                    )
                ));
                self.is_error = true;
            }
        }
        cx.notify();
    }
}

fn import_to_directory(
    source: ImportSource,
    path: &Path,
    directory: PathBuf,
    current_directory: &Path,
    language: LanguagePreference,
) -> anyhow::Result<ImportOutcome> {
    let directory = directory.canonicalize().context("无法解析目标目录")?;
    let current_directory = current_directory
        .canonicalize()
        .context("无法解析当前数据目录")?;
    if directory == current_directory {
        bail!(
            "{}",
            tr(
                language,
                "请选择新的空目录，不能恢复到当前运行的数据目录",
                "Choose a new empty folder, not the current running data folder"
            )
        );
    }
    // A failed import may leave only the platform's instance lock; allow retrying it.
    for entry in std::fs::read_dir(&directory).context("无法读取目标目录")? {
        let entry = entry?;
        if entry.file_name() != "instance.lock" || !entry.file_type()?.is_file() {
            bail!(
                "{}",
                tr(
                    language,
                    "目标目录非空，请选择新的空目录；不会覆盖或合并已有数据",
                    "The destination is not empty. Choose a new empty folder; existing data will not be overwritten or merged"
                )
            );
        }
    }
    let (items, attachments, missing) = match source {
        ImportSource::LegacyDatabase => {
            let (_, report) =
                clipboard_platform::import_legacy_data(path, Some(directory.clone()))?;
            (report.total_items, None, 0)
        }
        ImportSource::LegacyBackup => {
            let (_, report) =
                clipboard_platform::import_legacy_backup_data(path, directory.clone())?;
            (
                report.items.total_items,
                Some(report.images + report.icons + report.staged_files),
                report.unbundled_images,
            )
        }
        ImportSource::Backup => {
            let (_, report) = clipboard_platform::restore_backup_data(path, directory.clone())?;
            (
                report.total_items,
                Some(report.restored_images + report.restored_icons + report.restored_staged),
                report.missing_images + report.missing_icons + report.missing_staged,
            )
        }
    };
    Ok(ImportOutcome {
        directory,
        items,
        attachments,
        missing,
    })
}
