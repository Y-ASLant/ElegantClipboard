use super::super::UpdateStatus;
use super::{Event, Worker};
use anyhow::Result;
use std::path::PathBuf;

impl Worker {
    /// 查询 GitHub Releases 是否有同通道新版本。
    pub(super) fn handle_check_update(&mut self) -> Result<()> {
        let result = crate::update::fetch_latest(env!("CARGO_PKG_VERSION"))
            .map(|found| found.map_or(UpdateStatus::Current, UpdateStatus::Available))
            .map_err(|error| error.to_string());
        self.send_event(Event::UpdateChecked(result))
    }

    /// 下载 `release` 的安装包；成功路径回传给 UI 供 `InstallUpdate` 启动。
    pub(super) fn handle_download_update(
        &mut self,
        release: crate::update::ReleaseInfo,
    ) -> Result<()> {
        let result = crate::update::download_setup(&release, &self.data_dir.join("update"))
            .map_err(|error| error.to_string());
        self.send_event(Event::UpdateDownloaded(result))
    }

    /// 启动已下载的 NSIS 安装器（仅 Windows 真正执行）。
    pub(super) fn handle_install_update(&mut self, path: PathBuf) -> Result<()> {
        let result = crate::update::run_installer(&path).map_err(|error| error.to_string());
        self.send_event(Event::UpdateInstallStarted(result))
    }
}
