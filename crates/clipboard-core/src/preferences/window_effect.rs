use super::{
    DEFAULT_WINDOW_EFFECT_OPACITY, Preferences, WINDOW_EFFECT_KEY, WindowEffect,
    WindowEffectPreference,
};
use anyhow::Result;
use serde::Deserialize;
use std::borrow::Cow;

impl Preferences {
    pub fn window_effect(&self) -> Result<WindowEffectPreference> {
        let Some(value) = self.repository.get(WINDOW_EFFECT_KEY)? else {
            return Ok(WindowEffectPreference::default());
        };
        if let Ok(preference) = serde_json::from_str::<WindowEffectPreference>(&value) {
            return Ok(if preference.valid() {
                preference
            } else {
                WindowEffectPreference::default()
            });
        }
        let Some(preference) = migrate_legacy_preference(&value) else {
            return Ok(WindowEffectPreference::default());
        };
        self.write_json(WINDOW_EFFECT_KEY, &preference)?;
        Ok(preference)
    }

    pub fn set_window_effect(&self, preference: WindowEffectPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "窗口背景浓度超出允许范围");
        self.write_json(WINDOW_EFFECT_KEY, &preference)
    }
}

// Legacy spellings are accepted only at the storage boundary, never by the public types.
#[derive(Deserialize)]
#[serde(default)]
struct LegacyWindowEffectPreference<'a> {
    #[serde(borrow)]
    effect: Cow<'a, str>,
    opacity: u8,
}

impl Default for LegacyWindowEffectPreference<'_> {
    fn default() -> Self {
        Self {
            effect: Cow::Borrowed(""),
            opacity: DEFAULT_WINDOW_EFFECT_OPACITY,
        }
    }
}

fn migrate_legacy_preference(value: &str) -> Option<WindowEffectPreference> {
    let legacy: LegacyWindowEffectPreference<'_> = serde_json::from_str(value).ok()?;
    let effect = match legacy.effect.as_ref() {
        "none" => WindowEffect::Opaque,
        "acrylic" => WindowEffect::Frosted,
        "mica_alt" => WindowEffect::Mica,
        _ => return None,
    };
    let preference = WindowEffectPreference {
        effect,
        opacity: legacy.opacity,
    };
    preference.valid().then_some(preference)
}
