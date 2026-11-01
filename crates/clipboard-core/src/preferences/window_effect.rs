use super::{Preferences, WINDOW_EFFECT_KEY, WindowEffectPreference};
use anyhow::Result;

impl Preferences {
    pub fn window_effect(&self) -> Result<WindowEffectPreference> {
        let Some(value) = self.repository.get(WINDOW_EFFECT_KEY)? else {
            return Ok(WindowEffectPreference::default());
        };
        Ok(serde_json::from_str::<WindowEffectPreference>(&value)
            .ok()
            .filter(|preference| preference.valid())
            .unwrap_or_default())
    }

    pub fn set_window_effect(&self, preference: WindowEffectPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "窗口背景浓度超出允许范围");
        self.write_json(WINDOW_EFFECT_KEY, &preference)
    }
}
