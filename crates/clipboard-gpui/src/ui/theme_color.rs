use super::*;

impl ClipboardView {
    pub(super) fn apply_theme_color_saved(
        &mut self,
        result: Result<ThemeColor, String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.theme_color_pending = false;
        match result {
            Ok(color) => {
                self.close_hover_preview("theme_color_changed", cx);
                self.theme_color = color;
                crate::design_system::set_theme_color(color, cx);
                self.set_feedback_message(
                    tr(self.language, "主题色已保存", "Theme color saved").into(),
                );
                self.is_error = false;
            }
            Err(error) => {
                self.set_feedback_message(format!(
                    "{}: {error}",
                    tr(
                        self.language,
                        "主题色保存失败",
                        "Failed to save theme color"
                    )
                ));
                self.is_error = true;
            }
        }
        true
    }
}
