use super::*;
use anyhow::{Context as _, bail};

#[derive(Clone, Copy)]
pub(super) enum ImportSource {
    LegacyBackup,
    Backup,
}

pub(super) struct ImportOutcome {
    directory: PathBuf,
    items: i64,
    attachments: Option<usize>,
    missing: usize,
    blobs: usize,
    missing_blobs: usize,
}

impl ImportOutcome {
    pub(super) fn is_incomplete(&self) -> bool {
        self.missing != 0 || self.missing_blobs != 0
    }

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
        if let Some(count) = self.attachments {
            description.push_str(&if language == LanguagePreference::English {
                format!(
                    "; {count} image/icon attachments restored; {} missing",
                    self.missing
                )
            } else {
                format!(
                    "；已恢复 {count} 个图片或图标附件，缺失 {} 个",
                    self.missing
                )
            });
        }
        if self.blobs != 0 || self.missing_blobs != 0 {
            let blobs = self.blobs;
            let missing_blobs = self.missing_blobs;
            description.push_str(&if language == LanguagePreference::English {
                format!("; {blobs} native-format attachments restored; {missing_blobs} missing")
            } else {
                format!("；已恢复 {blobs} 个原生格式附件，缺失 {missing_blobs} 个")
            });
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
                    Ok(Some(outcome)) => this.data_import_result = Some(outcome),
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
            clipboard_platform::launch_env::apply(&mut command, Some(&outcome.directory));
            clipboard_platform::launch_env::apply_no_monitor(&mut command);
            command.spawn()
        });
        match result {
            Ok(_) => {
                self.set_feedback_message(
                    tr(
                        self.language,
                        "已请求预览恢复的数据，不记录剪贴板",
                        "Requested to preview restored data without recording the clipboard",
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
                        "无法预览恢复的数据，可重试",
                        "Unable to preview restored data. You can retry",
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
    let (items, attachments, missing, blobs, missing_blobs) = match source {
        ImportSource::LegacyBackup => {
            let (_, report) =
                clipboard_platform::import_legacy_backup_data(path, directory.clone())?;
            (
                report.items.total_items,
                Some(report.images + report.icons),
                report.unbundled_images,
                0,
                0,
            )
        }
        ImportSource::Backup => {
            let (_, report) = clipboard_platform::restore_backup_data(path, directory.clone())?;
            (
                report.total_items,
                Some(report.restored_images + report.restored_icons),
                report.missing_images + report.missing_icons,
                report.restored_blobs,
                report.missing_blobs,
            )
        }
    };
    Ok(ImportOutcome {
        directory,
        items,
        attachments,
        missing,
        blobs,
        missing_blobs,
    })
}

#[cfg(test)]
mod tests {
    use super::{ImportSource, import_to_directory};
    use clipboard_core::preferences::LanguagePreference;
    use clipboard_core::{History, NativeFormatCapture};

    #[test]
    fn backup_missing_only_native_blob_restores_as_incomplete() -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("source");
        let destination = temporary.path().join("destination");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&destination)?;
        let history = History::open(source.join("clipboard.db"))?;
        let id = history
            .capture("record with a missing native attachment")?
            .unwrap();
        let skipped = history.attach_native_formats(
            id,
            vec![NativeFormatCapture {
                name: "application/x-backup-test".into(),
                data: vec![42; 128 * 1024],
            }],
        )?;
        assert!(skipped.is_empty());
        std::fs::remove_dir_all(source.join("blobs"))?;

        let archive = temporary.path().join("backup.zip");
        let exported = history.export_backup(&archive)?;
        assert_eq!(exported.missing_images + exported.missing_icons, 0);
        assert_eq!((exported.included_blobs, exported.missing_blobs), (0, 1));

        let outcome = import_to_directory(
            ImportSource::Backup,
            &archive,
            destination,
            &source,
            LanguagePreference::English,
        )?;
        assert_eq!(outcome.items, 1);
        assert_eq!(outcome.missing, 0);
        assert_eq!((outcome.blobs, outcome.missing_blobs), (0, 1));
        assert!(outcome.is_incomplete());
        let restored = History::open(outcome.directory.join("clipboard.db"))?;
        assert_eq!(
            restored.text(id)?,
            "record with a missing native attachment"
        );
        Ok(())
    }
}
