//! 复制/粘贴前的文本转换（大小写、去空白）。core 纯函数，无平台依赖。

/// 可用的文本转换。顺序即菜单展示顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTransform {
    Trim,
    RemoveLineBreaks,
    Uppercase,
    Lowercase,
}

impl TextTransform {
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Trim => text.trim().to_owned(),
            Self::RemoveLineBreaks => text
                .split(['\r', '\n'])
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            Self::Uppercase => text.to_uppercase(),
            Self::Lowercase => text.to_lowercase(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextTransform::*;

    #[test]
    fn transforms_cover_the_common_special_paste_cases() {
        assert_eq!(Trim.apply("  中文 x  "), "中文 x");
        assert_eq!(RemoveLineBreaks.apply("a\r\nb\n\nc"), "a b c");
        assert_eq!(Uppercase.apply("Straße"), "STRASSE");
        assert_eq!(Lowercase.apply("ABC 中文"), "abc 中文");
    }
}
