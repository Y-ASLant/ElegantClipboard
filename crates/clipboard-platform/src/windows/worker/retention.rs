use super::{Event, Worker};
use anyhow::Result;
use clipboard_core::retention::RetentionPolicy;

impl Worker {
    pub(super) fn handle_set_retention(
        &mut self,
        policy: RetentionPolicy,
        confirmed: bool,
    ) -> Result<()> {
        let result = self
            .history
            .save_retention(policy, confirmed, &self.images_dir)
            .map(|outcome| (policy, outcome))
            .map_err(|error| error.to_string());
        let changed = matches!(&result, Ok((_, outcome)) if outcome.deleted > 0);
        self.send_event(Event::RetentionSaved(result))?;
        if changed {
            self.refresh_after_retention()?;
        }
        Ok(())
    }

    pub(in crate::windows) fn maintain_retention(&mut self) -> Result<()> {
        let outcome = self.history.apply_retention(&self.images_dir)?;
        if let Some(warning) = outcome.cleanup_warning {
            self.send_event(Event::BackgroundError(format!(
                "保留策略已执行，但附件回收不完整：{warning}"
            )))?;
        }
        if outcome.deleted > 0 {
            self.refresh_after_retention()?;
        }
        Ok(())
    }

    fn refresh_after_retention(&mut self) -> Result<()> {
        if let Err(error) = self.send_groups().and_then(|_| self.snapshot()) {
            self.send_event(Event::BackgroundError(format!(
                "保留策略已执行，但刷新历史失败：{error}"
            )))?;
        }
        Ok(())
    }
}
