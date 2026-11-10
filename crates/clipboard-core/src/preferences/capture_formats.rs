use serde::{Deserialize, Serialize};

const MAX_NAMES: usize = 256;
const MAX_NAME_CHARS: usize = 255;

const DEFAULT_NAMES: &[&str] = &[
    "CF_UNICODETEXT",
    "CF_TEXT",
    "CF_OEMTEXT",
    "CF_HDROP",
    "HTML Format",
    "Rich Text Format",
    "PNG",
    "CF_DIBV5",
    "CF_DIB",
    "JFIF",
    "UTF8_STRING",
    "STRING",
    "TEXT",
    "text/plain",
    "text/plain;charset=utf-8",
    "text/html",
    "text/rtf",
    "application/rtf",
    "text/uri-list",
    "image/png",
    "image/jpeg",
    "image/bmp",
    "image/tiff",
    "image/gif",
    "public.utf8-plain-text",
    "public.html",
    "public.rtf",
    "public.png",
    "public.jpeg",
    "public.tiff",
    "public.file-url",
];

const TEXT_NAMES: &[&str] = &[
    "CF_UNICODETEXT",
    "CF_TEXT",
    "CF_OEMTEXT",
    "UTF8_STRING",
    "STRING",
    "TEXT",
    "text/plain",
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "public.utf8-plain-text",
];
const HTML_NAMES: &[&str] = &["HTML Format", "text/html", "public.html"];
const RTF_NAMES: &[&str] = &[
    "Rich Text Format",
    "text/rtf",
    "application/rtf",
    "public.rtf",
];
const IMAGE_NAMES: &[&str] = &[
    "PNG",
    "CF_DIB",
    "CF_DIBV5",
    "JFIF",
    "image/png",
    "image/jpeg",
    "image/bmp",
    "image/tiff",
    "image/gif",
    "public.png",
    "public.jpeg",
    "public.tiff",
];
const FILE_NAMES: &[&str] = &["CF_HDROP", "text/uri-list", "public.file-url"];

/// 关闭「读取全部格式」时，只读取名单中的剪贴板格式名。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureFormatPreference {
    pub all: bool,
    pub names: Vec<String>,
}

impl Default for CaptureFormatPreference {
    fn default() -> Self {
        Self {
            all: false,
            names: DEFAULT_NAMES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }
}

impl CaptureFormatPreference {
    pub fn valid_name(name: &str) -> bool {
        let name = name.trim();
        !name.is_empty()
            && name.chars().count() <= MAX_NAME_CHARS
            && !name.chars().any(char::is_control)
    }

    pub fn normalized(mut self) -> Self {
        let mut names = Vec::with_capacity(self.names.len().min(MAX_NAMES));
        for name in self.names {
            let name = name.trim();
            if names.len() >= MAX_NAMES || !Self::valid_name(name) {
                continue;
            }
            if names
                .iter()
                .any(|item: &String| item.eq_ignore_ascii_case(name))
            {
                continue;
            }
            names.push(name.to_owned());
        }
        self.names = names;
        self
    }

    pub fn valid(&self) -> bool {
        self.names.len() <= MAX_NAMES && self.names.iter().all(|name| Self::valid_name(name))
    }

    pub fn includes(&self, name: &str) -> bool {
        self.all
            || self
                .names
                .iter()
                .any(|item| item.eq_ignore_ascii_case(name))
    }

    pub fn allows_text(&self) -> bool {
        self.allows_any(TEXT_NAMES)
    }

    pub fn allows_html(&self) -> bool {
        self.allows_any(HTML_NAMES)
    }

    pub fn allows_rtf(&self) -> bool {
        self.allows_any(RTF_NAMES)
    }

    pub fn allows_image(&self) -> bool {
        self.allows_any(IMAGE_NAMES)
    }

    pub fn allows_files(&self) -> bool {
        self.allows_any(FILE_NAMES)
    }

    fn allows_any(&self, names: &[&str]) -> bool {
        self.all || names.iter().any(|name| self.includes(name))
    }

    pub fn with_name(mut self, name: &str) -> Option<Self> {
        let name = name.trim();
        if !Self::valid_name(name) {
            return None;
        }
        if self
            .names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            return Some(self);
        }
        self.names.push(name.to_owned());
        self.valid().then_some(self)
    }

    pub fn without_name(mut self, name: &str) -> Self {
        self.names
            .retain(|existing| !existing.eq_ignore_ascii_case(name));
        self
    }

    pub fn with_default_names(self) -> Self {
        Self {
            names: Self::default().names,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CaptureFormatPreference;

    #[test]
    fn allowlist_matches_names_without_reading_everything() {
        let formats = CaptureFormatPreference {
            all: false,
            names: vec!["HTML Format".into(), "biff12".into()],
        };
        assert!(formats.includes("html format"));
        assert!(formats.includes("Biff12"));
        assert!(!formats.includes("CF_UNICODETEXT"));
        assert!(formats.allows_html());
        assert!(!formats.allows_text());
        assert!(formats.clone().with_name("  html format  ").unwrap() == formats);
        assert!(formats.clone().with_name("bad\nname").is_none());
        assert_eq!(
            formats.clone().without_name("BIFF12").names,
            vec!["HTML Format".to_owned()]
        );
        assert!(CaptureFormatPreference::default().allows_text());
        assert!(
            !CaptureFormatPreference {
                all: true,
                names: vec!["bad\nname".into(), "  ".into(), "PNG".into()],
            }
            .normalized()
            .names
            .iter()
            .any(|name| name != "PNG")
        );
    }
}
