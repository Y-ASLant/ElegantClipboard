use serde::{Deserialize, Serialize};

use crate::database::ContentType;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoverPreviewPosition {
    #[default]
    Auto,
    Left,
    Right,
}

/// Which hover-information chips are drawn. Missing saved values stay visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverPreviewDetails {
    pub kind: bool,
    pub size: bool,
    pub resolution: bool,
    pub amount: bool,
    pub name: bool,
    pub path: bool,
    pub extension: bool,
    pub formats: bool,
    pub source: bool,
    pub captured_at: bool,
    pub hint: bool,
}

impl Default for HoverPreviewDetails {
    fn default() -> Self {
        Self {
            kind: true,
            size: true,
            resolution: true,
            amount: true,
            name: true,
            path: true,
            extension: true,
            formats: true,
            source: true,
            captured_at: true,
            hint: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverPreviewPreference {
    pub image: bool,
    pub text: bool,
    pub files: bool,
    pub expanded_image: bool,
    pub delay_ms: u16,
    pub position: HoverPreviewPosition,
    pub zoom_step: u8,
    #[serde(default)]
    pub details: HoverPreviewDetails,
}

impl Default for HoverPreviewPreference {
    fn default() -> Self {
        Self {
            image: true,
            text: true,
            files: false,
            expanded_image: false,
            delay_ms: 500,
            position: HoverPreviewPosition::Auto,
            zoom_step: 10,
            details: HoverPreviewDetails::default(),
        }
    }
}

impl HoverPreviewPreference {
    pub fn allows(self, content_type: &str) -> bool {
        match ContentType::parse(content_type) {
            Some(ContentType::Image) => self.image,
            Some(ContentType::Files) => self.files,
            Some(kind) if kind.is_text_like() => self.text,
            _ => false,
        }
    }

    pub(super) fn valid(self) -> bool {
        (100..=2000).contains(&self.delay_ms) && (5..=50).contains(&self.zoom_step)
    }
}
