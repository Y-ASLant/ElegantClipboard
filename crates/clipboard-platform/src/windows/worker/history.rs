use super::{Event, Worker};
use anyhow::{Context, Result, anyhow};

impl Worker {
    /// 列表视图是否仍是命令发起时的视图（分组与代次一致）。
    fn view_is_current(&self, group_id: Option<i64>, generation: u64) -> bool {
        group_id == self.group_id && generation == self.generation
    }

    pub(super) fn handle_delete_batch(
        &mut self,
        ids: Vec<i64>,
        group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result = if self.view_is_current(group_id, generation) {
            self.history
                .delete_batch_with_media(&ids, group_id, &self.images_dir)
        } else {
            Err(anyhow!("列表已切换，请重新选择要删除的记录"))
        };
        let refresh = if result.is_ok() {
            self.send_groups()
                .and_then(|_| self.snapshot())
                .context("所选记录已删除，但列表或分组计数刷新失败，请重启应用后查看")
        } else {
            Ok(())
        };
        self.send_event(Event::BatchDeleted(
            result.map_err(|error| error.to_string()),
        ))?;
        if let Err(error) = refresh {
            self.send_event(Event::BackgroundError(error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn handle_clear_history(
        &mut self,
        group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result = if self.view_is_current(group_id, generation) {
            self.history
                .clear_history_with_media(group_id, &self.images_dir)
        } else {
            Err(anyhow!("列表已切换，请重新选择要清理的分组"))
        };
        let refresh = if result.is_ok() {
            self.send_groups()
                .and_then(|_| self.snapshot())
                .context("历史记录已清理，但列表或分组计数刷新失败，请重启应用后查看")
        } else {
            Ok(())
        };
        self.send_event(Event::HistoryCleared(
            result.map_err(|error| error.to_string()),
        ))?;
        if let Err(error) = refresh {
            self.send_event(Event::BackgroundError(error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn handle_clear_all(&mut self) -> Result<()> {
        let result = self.history.clear_all_with_media(&self.images_dir);
        let refresh = if result.is_ok() {
            self.send_groups()
                .and_then(|_| self.snapshot())
                .context("全部历史已删除，但列表或分组计数刷新失败，请重启应用后查看")
        } else {
            Ok(())
        };
        self.send_event(Event::AllHistoryCleared(
            result.map_err(|error| error.to_string()),
        ))?;
        if let Err(error) = refresh {
            self.send_event(Event::BackgroundError(error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn handle_reorder(
        &mut self,
        from: i64,
        to: i64,
        after: bool,
        favorite_only: bool,
        group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result =
            if self.view_is_current(group_id, generation) && favorite_only == self.favorite_only {
                self.history
                    .reorder_in_group(from, to, after, favorite_only, group_id)
            } else {
                Err(anyhow!("列表已切换，请重新拖动"))
            };
        let result = result.and_then(|_| {
            self.snapshot()
                .context("顺序已保存，但列表刷新失败，请重启应用后查看")
        });
        self.send_event(Event::Reordered {
            from,
            generation,
            result: result.map_err(|error| error.to_string()),
        })
    }

    pub(super) fn handle_create_group(&mut self, name: String) -> Result<()> {
        let result = self
            .history
            .create_group(&name)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.send_groups()?;
        }
        self.send_event(Event::GroupCreated(result))
    }

    pub(super) fn handle_reorder_group(&mut self, from: i64, to: i64, after: bool) -> Result<()> {
        let result = self.history.reorder_group(from, to, after).and_then(|_| {
            self.send_groups()
                .context("分组顺序已保存，但列表刷新失败，请重启应用后查看")
        });
        self.send_event(Event::GroupReordered {
            from,
            result: result.map_err(|error| error.to_string()),
        })
    }

    pub(super) fn handle_rename_group(&mut self, id: i64, name: String) -> Result<()> {
        let result = self
            .history
            .rename_group(id, &name)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.send_groups()?;
        }
        self.send_event(Event::GroupRenamed(result))
    }

    pub(super) fn handle_delete_group(&mut self, id: i64, generation: u64) -> Result<()> {
        let result = if self.view_is_current(Some(id), generation) {
            self.history.delete_group_preserving_items(id)
        } else {
            Err(anyhow!("列表已切换，请重新选择分组"))
        }
        .map_err(|error| error.to_string());
        if result.is_ok() {
            self.group_id = None;
            self.send_groups()?;
        }
        self.send_event(Event::GroupDeleted(result))
    }

    pub(super) fn handle_move_to_group(
        &mut self,
        id: i64,
        source_group_id: Option<i64>,
        target_group_id: Option<i64>,
        generation: u64,
    ) -> Result<()> {
        let result = if self.view_is_current(source_group_id, generation) {
            self.history
                .move_to_group(id, source_group_id, target_group_id)
        } else {
            Err(anyhow!("列表已切换，请重新选择记录"))
        };
        if result.is_ok() {
            self.send_groups()?;
            self.snapshot()?;
        }
        self.send_event(Event::ItemMoved {
            id,
            result: result.map_err(|error| error.to_string()),
        })
    }
}
