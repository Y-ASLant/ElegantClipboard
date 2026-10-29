//! 「关于」页项目链接；三平台共用的枚举与 URL 常量。
//!
//! URL 只在此处定义为编译期常量；各平台的 `native::links::open`
//! 是唯一把它们交给系统打开器的入口。

/// 「关于」页可打开的项目链接。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectLink {
    Author,
    Repository,
    Issues,
}

impl ProjectLink {
    pub fn url(self) -> &'static str {
        match self {
            Self::Author => "https://github.com/Y-ASLant",
            Self::Repository => "https://github.com/Y-ASLant/ElegantClipboard",
            Self::Issues => "https://github.com/Y-ASLant/ElegantClipboard/issues",
        }
    }
}
