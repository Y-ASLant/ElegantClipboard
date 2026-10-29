use clipboard_core::TextTransform;

/// 复制模式分派：worker 收到复制命令后按此选择写入剪贴板的内容集合。
/// 纯逻辑枚举，不含任何平台 API；各平台模块负责实际写入。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyMode {
    Original,
    PlainText,
    Paths,
    Files,
    /// 富文本条目的单一格式复制（HTML 或 RTF），缺少该格式时报错不降级。
    Html,
    Rtf,
    /// 文本转换后的纯文本复制（见决策 0003）。
    Transform(TextTransform),
}

impl CopyMode {
    pub(super) fn from(format: crate::service::protocol::CopyFormat) -> Self {
        match format {
            crate::service::protocol::CopyFormat::Html => Self::Html,
            crate::service::protocol::CopyFormat::Rtf => Self::Rtf,
        }
    }
}
