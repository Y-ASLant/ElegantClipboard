use super::{Command, Event, capture::CaptureState};
use anyhow::{Result, anyhow};
use clipboard_core::{
    ContentCategory, History,
    preferences::{AppFilterPreference, MonitorTypesPreference, Preferences},
    search::search_excerpt,
};
use clipboard_rs::ClipboardContext;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

mod capture;
mod clipboard;
mod clipboard_write;
mod commands;
mod history;
mod image_tmp;
mod retention;
mod settings;
pub(crate) mod transient;

#[cfg(test)]
pub(super) use capture::filter_observed_capture;

pub(super) struct Worker {
    pub(super) history: History,
    pub(super) preferences: Preferences,
    pub(super) clipboard: ClipboardContext,
    pub(super) images_dir: PathBuf,
    pub(super) staged_dir: PathBuf,
    pub(super) icons_dir: PathBuf,
    pub(super) data_dir: PathBuf,
    pub(super) state: Arc<Mutex<CaptureState>>,
    pub(super) monitor_types: MonitorTypesPreference,
    pub(super) app_filter: AppFilterPreference,
    pub(super) transient: transient::TransientWindow,
    pub(super) search: String,
    pub(super) favorite_only: bool,
    pub(super) category: ContentCategory,
    pub(super) group_id: Option<i64>,
    pub(super) limit: i64,
    pub(super) generation: u64,
    pub(super) snapshot_pending: bool,
    pub(super) events: async_channel::Sender<Event>,
}

/// 瞬时确认窗口的重读入口（service.rs 的 deliver_transient_due 调用）。
pub(super) fn clipboard_read_current(
    clipboard: &ClipboardContext,
) -> anyhow::Result<Option<super::CapturedClipboard>> {
    super::read_formats::read_current_content(clipboard)
}

impl Worker {
    /// 启动清扫：blob pending 残留与过期图片临时文件。
    pub(super) fn sweep_temp_media(&self) -> Result<()> {
        crate::windows::worker::image_tmp::sweep_image_tmp(
            &self.data_dir,
            std::time::Instant::now(),
        )?;
        let blob_root = clipboard_core::blob_root_for(&self.data_dir.join("clipboard.db"))
            .ok_or_else(|| anyhow!("无法确定 blob 存储目录"))?;
        clipboard_core::blob_store_public::sweep_pending_at(&blob_root)
    }

    /// Blocking event delivery; the only failure mode is a closed UI receiver.
    fn send_event(&self, event: Event) -> Result<()> {
        self.events
            .send_blocking(event)
            .map_err(|_| anyhow!("窗口已关闭"))
    }

    pub(super) fn send_groups(&self) -> Result<()> {
        self.send_event(Event::Groups(self.history.groups()?))
    }

    pub(super) fn snapshot(&mut self) -> Result<()> {
        let event = self.snapshot_event()?;
        self.snapshot_pending = false;
        self.send_event(event)
    }

    pub(super) fn flush_capture_snapshot(&mut self) -> Result<()> {
        if !self.snapshot_pending || self.events.is_full() || self.events.is_closed() {
            return Ok(());
        }
        let event = self.snapshot_event()?;
        match self.events.try_send(event) {
            Ok(()) | Err(async_channel::TrySendError::Closed(_)) => {
                self.snapshot_pending = false;
            }
            Err(async_channel::TrySendError::Full(_)) => {}
        }
        Ok(())
    }

    fn snapshot_event(&self) -> Result<Event> {
        let mut items = self
            .history
            .list_filtered_in_group(
                &self.search,
                self.limit,
                self.favorite_only,
                self.group_id,
                self.category,
            )
            .map_err(|error| anyhow!("读取历史记录失败：{error}"))?;
        for item in &mut items {
            if let Some(full) = item.text_content.take()
                && item.content_type != "image"
                && let Some(excerpt) = search_excerpt(
                    &full,
                    item.preview.as_deref().unwrap_or_default(),
                    &self.search,
                )
            {
                item.preview = Some(excerpt);
            }
        }
        Ok(Event::Snapshot {
            items,
            total: self.history.count_filtered_in_group(
                &self.search,
                self.favorite_only,
                self.group_id,
                self.category,
            )?,
            generation: self.generation,
        })
    }
}
