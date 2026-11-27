use super::{CapturedClipboard, Command, Event, capture::CaptureState};
use anyhow::{Result, anyhow};
use clipboard_core::{
    ContentCategory, History,
    database::ClipboardItem,
    preferences::{AppFilterPreference, MonitorTypesPreference, Preferences},
    search::search_excerpt,
};
use clipboard_rs::ClipboardContext;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

mod capture;
#[cfg(windows)]
#[path = "../../windows/worker/clipboard.rs"]
mod clipboard;
#[cfg(unix)]
#[path = "../../unix/clipboard.rs"]
mod clipboard;
mod clipboard_content;
#[cfg(windows)]
#[path = "../../windows/worker/clipboard_restore.rs"]
mod clipboard_restore;
#[cfg(windows)]
#[path = "../../windows/worker/clipboard_write.rs"]
mod clipboard_write;
mod commands;
mod history;
mod image_tmp;
mod retention;
mod settings;
mod stash;
pub(crate) mod transient;
mod update;

use stash::StashedClipboard;

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
    /// 受控粘贴前暂存的用户剪贴板（决策 0003）；仅在开启恢复时写入。
    pub(super) clipboard_stash: Option<StashedClipboard>,
    pub(super) search: String,
    pub(super) favorite_only: bool,
    pub(super) category: ContentCategory,
    pub(super) group_id: Option<i64>,
    pub(super) limit: i64,
    pub(super) offset: i64,
    pub(super) generation: u64,
    pub(super) snapshot_pending: bool,
    pub(super) events: async_channel::Sender<Event>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SnapshotReason {
    Page,
    Scope,
    Mutation,
}

impl Worker {
    /// 启动清扫：图片临时文件、blob pending，以及库中已无引用的正式 blob。
    pub(super) fn sweep_temp_media(&self) -> Result<()> {
        image_tmp::sweep_image_tmp(&self.data_dir)?;
        self.history.sweep_blob_store()
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
        self.send_snapshot(SnapshotReason::Mutation)
    }

    fn send_snapshot(&mut self, reason: SnapshotReason) -> Result<()> {
        let event = self.snapshot_event(reason)?;
        self.snapshot_pending = false;
        self.send_event(event)
    }

    pub(super) fn flush_capture_snapshot(&mut self) -> Result<()> {
        if !self.snapshot_pending || self.events.is_full() || self.events.is_closed() {
            return Ok(());
        }
        let event = self.snapshot_event(SnapshotReason::Mutation)?;
        match self.events.try_send(event) {
            Ok(()) | Err(async_channel::TrySendError::Closed(_)) => {
                self.snapshot_pending = false;
            }
            Err(async_channel::TrySendError::Full(_)) => {}
        }
        Ok(())
    }

    fn snapshot_event(&self, reason: SnapshotReason) -> Result<Event> {
        let structural = reason == SnapshotReason::Mutation;
        // Captures insert at the top and shift every position, so a structural
        // refresh re-reads from row zero: no earlier window survives intact.
        let offset = if structural { 0 } else { self.offset };
        let (items, total) =
            self.read_page(self.favorite_only, self.category, offset, self.limit)?;
        // Keep the return page current without changing the worker's active filter.
        // Scrolling Favorites alone cannot change All and needs no companion query.
        let all_page = (self.favorite_only && reason != SnapshotReason::Page)
            .then(|| {
                self.read_page(
                    false,
                    ContentCategory::All,
                    0,
                    clipboard_core::HISTORY_PAGE_ROWS as i64,
                )
            })
            .transpose()?;
        Ok(Event::Snapshot {
            items,
            total,
            offset,
            structural,
            all_page,
            generation: self.generation,
        })
    }

    fn read_page(
        &self,
        favorite_only: bool,
        category: ContentCategory,
        offset: i64,
        limit: i64,
    ) -> Result<(Vec<ClipboardItem>, i64)> {
        let mut items = self
            .history
            .list_page_in_group(
                &self.search,
                limit,
                offset,
                favorite_only,
                self.group_id,
                category,
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
        let total = self.history.count_filtered_in_group(
            &self.search,
            favorite_only,
            self.group_id,
            category,
        )?;
        Ok((items, total))
    }
}
