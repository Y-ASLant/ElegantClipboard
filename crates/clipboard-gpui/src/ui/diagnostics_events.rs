use super::*;

impl ClipboardView {
    pub(super) fn apply_diagnostics_enabled_saved(&mut self, result: Result<bool, String>) -> bool {
        if let Ok(enabled) = result.as_ref()
            && let Some(diagnostics) = &self.diagnostics
        {
            diagnostics.set_enabled(*enabled);
        }
        self.acknowledge_save(
            result,
            |this| &mut this.diagnostics_enabled,
            ("保存日志记录设置失败", "Failed to save diagnostics setting"),
        );
        true
    }

    pub(super) fn apply_run_as_admin_saved(&mut self, result: Result<bool, String>) -> bool {
        let notice = match result.as_ref() {
            Ok(true) => Some((
                "已开启管理员启动，下次启动生效",
                "Admin launch enabled; takes effect on next start",
            )),
            Ok(false) => Some((
                "已关闭管理员启动，下次启动生效",
                "Admin launch disabled; takes effect on next start",
            )),
            Err(_) => None,
        };
        self.acknowledge_save(
            result,
            |this| &mut this.run_as_admin,
            (
                "保存管理员启动设置失败",
                "Failed to save admin launch setting",
            ),
        );
        if let Some((chinese, english)) = notice {
            self.set_feedback_message(tr(self.language, chinese, english).into());
            self.is_error = false;
        }
        true
    }
}
