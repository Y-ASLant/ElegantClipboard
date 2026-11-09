use super::{FONT_KEY, FontPreference, Preferences};
use anyhow::Result;

impl Preferences {
    pub fn fonts(&self) -> Result<FontPreference> {
        self.read_json(FONT_KEY, FontPreference::valid)
    }

    pub fn set_fonts(&self, preference: FontPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "字体设置超出允许范围");
        self.write_json(FONT_KEY, &preference)
    }
}
