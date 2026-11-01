use super::*;

/// 「检查更新」状态机：检查 → 下载 → 安装。失败原因经 feedback 条提示。
#[derive(Debug)]
pub(super) enum UpdateCheck {
    Idle,
    #[cfg(windows)]
    Checking,
    UpToDate,
    Available(ReleaseInfo),
    Downloading,
    Ready(PathBuf),
    Failed,
}

impl ClipboardView {
    #[cfg(windows)]
    pub(super) fn start_update_check(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update_check,
            UpdateCheck::Checking | UpdateCheck::Downloading
        ) {
            return;
        }
        if self.send(Command::CheckUpdate, cx) {
            self.update_check = UpdateCheck::Checking;
            cx.notify();
        }
    }

    pub(super) fn start_update_download(&mut self, cx: &mut Context<Self>) {
        let UpdateCheck::Available(release) = &self.update_check else {
            return;
        };
        let release = release.clone();
        if self.send(Command::DownloadUpdate(release), cx) {
            self.update_check = UpdateCheck::Downloading;
            cx.notify();
        }
    }

    pub(super) fn start_update_install(&mut self, cx: &mut Context<Self>) {
        let UpdateCheck::Ready(path) = &self.update_check else {
            return;
        };
        let path = path.clone();
        // 不在这里退出：worker 启动安装器成功后经 apply_update_install_started 退出，
        // 失败时 app 还活着能展示错误。
        self.send(Command::InstallUpdate(path), cx);
    }

    pub(super) fn apply_update_checked(&mut self, result: Result<UpdateStatus, String>) -> bool {
        match result {
            Ok(UpdateStatus::Current) => self.update_check = UpdateCheck::UpToDate,
            Ok(UpdateStatus::Available(release)) => {
                self.update_check = UpdateCheck::Available(release)
            }
            Err(error) => self.fail_update(error),
        }
        true
    }

    pub(super) fn apply_update_downloaded(
        &mut self,
        result: Result<std::path::PathBuf, String>,
    ) -> bool {
        match result {
            Ok(path) => self.update_check = UpdateCheck::Ready(path),
            Err(error) => self.fail_update(error),
        }
        true
    }

    pub(super) fn apply_update_install_started(
        &mut self,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            // 安装器已接管，退出当前实例让它替换文件。
            Ok(()) => {
                self.exiting.set(true);
                cx.quit();
            }
            Err(error) => self.fail_update(error),
        }
        true
    }

    fn fail_update(&mut self, error: String) {
        self.update_check = UpdateCheck::Failed;
        self.set_feedback_message(format!(
            "{}: {error}",
            tr(self.language, "检查更新失败", "Update failed")
        ));
        self.is_error = true;
    }
}
