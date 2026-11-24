use super::*;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportSource {
    LegacyBackup,
    Backup,
}

/// 导入流程阶段：路径选择 vs 实际解压/升级/安装。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportPhase {
    ChoosingPaths(ImportSource),
    Importing(ImportSource),
}

pub(super) struct ImportOutcome {
    directory: PathBuf,
    items: i64,
    attachments: Option<usize>,
    missing: usize,
    blobs: usize,
    missing_blobs: usize,
    /// 旧版 ZIP 只导入历史；GPUI 备份完整恢复含设置。
    settings_restored: bool,
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
        description.push_str(if self.settings_restored {
            tr(
                language,
                "；设置已一并恢复。",
                "; settings restored with the backup.",
            )
        } else {
            tr(
                language,
                "；未导入任何设置，打开后使用默认设置。",
                "; no settings imported — defaults apply when opened.",
            )
        });
        description
    }
}

impl ClipboardView {
    /// 仅选 ZIP：导入/恢复到**当前数据保存位置**，确认后排队并重启应用。
    /// 必须在按钮 on_click 里直接调用（不要包一层 entity.update），避免 dialog 二次 update 闪退。
    pub(super) fn start_data_import(
        owner: WeakEntity<Self>,
        source: ImportSource,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(view) = owner.upgrade() else {
            return;
        };
        if view.read(cx).data_import_phase.is_some() {
            return;
        }
        let language = view.read(cx).language;
        let title = match source {
            ImportSource::LegacyBackup => tr(
                language,
                "导入到当前保存位置？",
                "Import into current save location?",
            ),
            ImportSource::Backup => tr(
                language,
                "恢复到当前保存位置？",
                "Restore into current save location?",
            ),
        };
        let body = tr(
            language,
            "将清空并替换当前保存位置中的历史与媒体，然后自动重启。",
            "Clears and replaces history and media in the current save location, then restarts.",
        );
        let detail = tr(
            language,
            "如需改路径，先到「本地数据」。",
            "To change the path, go to Local data first.",
        );
        open_settings_danger_dialog(
            window,
            cx,
            owner,
            SettingsDangerDialog {
                title,
                body,
                detail,
                cancel_id: "import-current-cancel",
                confirm_id: "import-current-confirm",
                error_id: "import-current-error",
                cancel_label: tr(language, "取消", "Cancel"),
                confirm_label: tr(language, "选择备份", "Choose backup"),
                pending_label: tr(language, "正在准备", "Preparing"),
            },
            |view| {
                view.data_import_phase = None;
            },
            |view| {
                (
                    matches!(view.data_import_phase, Some(ImportPhase::Importing(_))),
                    None,
                )
            },
            move |view, cx| {
                view.pick_import_archive_and_restart(source, cx);
            },
        );
    }

    fn pick_import_archive_and_restart(&mut self, source: ImportSource, cx: &mut Context<Self>) {
        if matches!(self.data_import_phase, Some(ImportPhase::Importing(_))) {
            return;
        }
        self.data_import_phase = Some(ImportPhase::ChoosingPaths(source));
        let language = self.language;
        let data_dir = self.service.data_dir.clone();
        let kind = match source {
            ImportSource::LegacyBackup => "legacy",
            ImportSource::Backup => "gpui",
        };
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
            let picked = selection.await;
            let ready = view.update(cx, |this, cx| {
                this.data_import_phase = None;
                match picked {
                    Ok(Ok(paths)) => paths.and_then(|paths| paths.into_iter().next()),
                    Ok(Err(error)) => {
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(this.language, "导入 / 恢复失败", "Import / restore failed")
                        ));
                        this.is_error = true;
                        cx.notify();
                        None
                    }
                    Err(error) => {
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(this.language, "导入 / 恢复失败", "Import / restore failed")
                        ));
                        this.is_error = true;
                        cx.notify();
                        None
                    }
                }
            });
            let Ok(Some(path)) = ready else {
                return;
            };
            let queued = view.update(cx, |this, cx| {
                this.data_import_phase = Some(ImportPhase::Importing(source));
                this.set_feedback_message(
                    tr(
                        this.language,
                        "已排队导入，即将重启并显示进度",
                        "Import queued. Restarting with a progress window",
                    )
                    .into(),
                );
                this.is_error = false;
                cx.notify();
                if let Err(error) = clipboard_platform::queue_pending_import(&data_dir, &path, kind)
                {
                    this.set_feedback_message(format!(
                        "{}: {error:#}",
                        tr(this.language, "导入 / 恢复失败", "Import / restore failed")
                    ));
                    this.is_error = true;
                    this.data_import_phase = None;
                    cx.notify();
                    return false;
                }
                true
            });
            if !matches!(queued, Ok(true)) {
                return;
            }
            // 稍留进度文案，避免“点完直接没了”的假闪退感。
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;
            let _ = view.update(cx, |this, cx| {
                this.restart_into_data_dir(None, cx);
            });
        })
        .detach();
    }

    /// 必须在按钮 on_click 里直接调用（不要包一层 entity.update）。
    pub(super) fn start_change_data_directory(
        owner: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(view) = owner.upgrade() else {
            return;
        };
        if view.read(cx).data_import_phase.is_some() {
            return;
        }
        let language = view.read(cx).language;
        open_settings_danger_dialog(
            window,
            cx,
            owner,
            SettingsDangerDialog {
                title: tr(language, "更改数据保存位置？", "Change data save location?"),
                body: tr(
                    language,
                    "选择空目录后，当前历史会移动过去，并重启到新位置。",
                    "After you pick an empty folder, current history moves there and the app restarts.",
                ),
                detail: tr(
                    language,
                    "请选择空目录。现有历史会移动到新位置。",
                    "Pick an empty folder. Existing history moves to the new location.",
                ),
                cancel_id: "relocate-cancel",
                confirm_id: "relocate-confirm",
                error_id: "relocate-error",
                cancel_label: tr(language, "取消", "Cancel"),
                confirm_label: tr(language, "选择目录", "Choose folder"),
                pending_label: tr(language, "正在准备", "Preparing"),
            },
            |_| {},
            |_| (false, None),
            |view, cx| view.pick_data_directory_and_restart(cx),
        );
    }

    fn pick_data_directory_and_restart(&mut self, cx: &mut Context<Self>) {
        let language = self.language;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                tr(
                    language,
                    "选择新的空数据目录",
                    "Select a new empty data folder",
                )
                .into(),
            ),
        });
        self.data_import_phase = Some(ImportPhase::ChoosingPaths(ImportSource::Backup));
        cx.notify();
        cx.spawn(async move |view, cx| {
            let picked = selection.await;
            let path = match view.update(cx, |this, cx| {
                this.data_import_phase = None;
                match picked {
                    Ok(Ok(paths)) => paths.and_then(|paths| paths.into_iter().next()),
                    Ok(Err(error)) => {
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "更改数据目录失败",
                                "Failed to change data folder"
                            )
                        ));
                        this.is_error = true;
                        cx.notify();
                        None
                    }
                    Err(error) => {
                        this.set_feedback_message(format!(
                            "{}: {error}",
                            tr(
                                this.language,
                                "更改数据目录失败",
                                "Failed to change data folder"
                            )
                        ));
                        this.is_error = true;
                        cx.notify();
                        None
                    }
                }
            }) {
                Ok(Some(path)) => path,
                _ => return,
            };
            let queued = view.update(cx, |this, cx| {
                if let Err(error) = clipboard_platform::queue_pending_relocate(&path) {
                    this.set_feedback_message(format!(
                        "{}: {error:#}",
                        tr(
                            this.language,
                            "更改数据目录失败",
                            "Failed to change data folder"
                        )
                    ));
                    this.is_error = true;
                    cx.notify();
                    return false;
                }
                true
            });
            if !matches!(queued, Ok(true)) {
                return;
            }
            // 不绑定旧 DATA_DIR，启动时执行 relocate 并读新 location。
            let _ = view.update(cx, |this, cx| {
                this.restart_into_data_dir(Some(false), cx);
            });
        })
        .detach();
    }

    /// `bind_current = None/true`：绑定当前 data_dir；`Some(false)`：清除 DATA_DIR，读 location 指针。
    pub(super) fn restart_into_data_dir(
        &mut self,
        bind_current: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let data_dir = self.service.data_dir.clone();
        let bind = bind_current.unwrap_or(true);
        // 子进程需等本进程放锁；标记写在当前目录，迁址时再写一份到控制根（路径切换后仍能读到）。
        if let Err(error) = clipboard_platform::launch_env::mark_wait_for_instance(&data_dir) {
            self.set_feedback_message(format!(
                "{}: {error}",
                tr(self.language, "重启失败", "Failed to restart")
            ));
            self.is_error = true;
            self.data_import_phase = None;
            cx.notify();
            return;
        }
        if !bind && let Ok(root) = clipboard_platform::control_root() {
            let _ = clipboard_platform::launch_env::mark_wait_for_instance(&root);
        }
        let spawn = std::env::current_exe().and_then(|executable| {
            let mut command = std::process::Command::new(executable);
            if bind {
                clipboard_platform::launch_env::apply(&mut command, Some(&data_dir));
            } else {
                clipboard_platform::launch_env::apply(&mut command, None);
            }
            command.spawn()
        });
        match spawn {
            Ok(_) => {
                self.exiting.set(true);
                cx.quit();
            }
            Err(error) => {
                let _ = clipboard_platform::launch_env::take_wait_for_instance(&data_dir);
                if !bind {
                    if let Ok(root) = clipboard_platform::control_root() {
                        let _ = clipboard_platform::launch_env::take_wait_for_instance(&root);
                        let _ = std::fs::remove_file(root.join(".pending-relocate"));
                    }
                } else {
                    let _ = std::fs::remove_file(data_dir.join(".pending-import"));
                }
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(self.language, "重启失败", "Failed to restart")
                ));
                self.is_error = true;
                self.data_import_phase = None;
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
use anyhow::{Context as _, bail};

#[cfg(test)]
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
        settings_restored: matches!(source, ImportSource::Backup),
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
