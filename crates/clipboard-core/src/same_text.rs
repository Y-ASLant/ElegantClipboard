mod merge;

#[cfg(test)]
mod tests;

use crate::database::ContentType;

/// 同文合并的输入：本次采集的期望类型、富文本表示与原始内容哈希。
/// 合并只对有纯文本表示的采集发生，text 恒有值。
pub(crate) struct MergeInput<'a> {
    pub content_type: ContentType,
    pub html: Option<&'a str>,
    pub rtf: Option<&'a [u8]>,
    pub text: &'a str,
    pub hash: String,
}

/// 类型升级规则：仅纯文本条目可升级为富文本/网址；url 与富文本目标保持类型不变。
pub(crate) fn upgraded_type(current: &str, incoming: ContentType) -> ContentType {
    if current != "text" {
        return ContentType::parse(current).unwrap_or_default();
    }
    match incoming {
        ContentType::Text | ContentType::Url | ContentType::Html | ContentType::Rtf => incoming,
        _ => ContentType::Text,
    }
}
