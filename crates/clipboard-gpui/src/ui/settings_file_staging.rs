use super::settings_common::command_checkbox;
use super::*;

impl SettingsWindowView {
    pub(super) fn render_file_staging_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let owner = owner.read(cx);
        let language = owner.language;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(command_checkbox(
                "stage-files",
                tr(language, "保存普通文件副本（默认关闭）", "Keep file copies (off by default)"),
                owner.stage_files.value,
                owner.stage_files.pending,
                &self.owner,
                Command::SetStageFiles,
                |owner| owner.stage_files.pending = true,
            ))
            .child(div().text_xs().child(tr(
                language,
                "仅对之后捕获的普通文件创建副本：单文件最多 50 MiB，暂存目录总预算 256 MiB；不复制文件夹、符号链接或重解析路径。超限或失败仍保存原路径并提示警告。",
                "Copies future regular files only: up to 50 MiB per file and 256 MiB total. Folders, symbolic links and reparse paths are excluded. Limits or failures keep original paths and show a warning.",
            )))
            .child(div().text_xs().child(tr(
                language,
                "优先使用原文件，仅在原文件丢失时使用副本。关闭后保留已有副本，删除最后一条引用记录时回收。",
                "Original files are preferred; copies are used when originals are missing. Turning this off keeps existing copies until their last history reference is deleted.",
            )))
            .into_any_element()
    }
}

impl ClipboardView {
    pub(super) fn apply_stage_files_saved(&mut self, result: Result<bool, String>) -> bool {
        self.acknowledge_save(
            result,
            |this| &mut this.stage_files,
            ("文件副本设置已保存", "File copy setting saved"),
            ("保存文件副本设置失败", "Failed to save file copy setting"),
        );
        true
    }
}
