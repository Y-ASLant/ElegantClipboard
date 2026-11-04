use super::{
    CARD_CORNER_KEY, CORNER_RADIUS_KEY, CardCornerPreference, CornerRadiusPreference, Preferences,
};
use anyhow::Result;

impl Preferences {
    pub fn corner_radius(&self) -> Result<CornerRadiusPreference> {
        let stored = self.repository.get(CORNER_RADIUS_KEY)?;
        if stored.as_deref() == Some("system") {
            return Ok(CornerRadiusPreference::System);
        }
        Ok(match stored.and_then(|value| value.parse::<u8>().ok()) {
            Some(radius @ 0..=16) => CornerRadiusPreference::Fixed(radius),
            _ => CornerRadiusPreference::default(),
        })
    }

    pub fn set_corner_radius(&self, preference: CornerRadiusPreference) -> Result<()> {
        match preference {
            CornerRadiusPreference::System => {
                self.repository.set(CORNER_RADIUS_KEY, "system")?;
            }
            CornerRadiusPreference::Fixed(radius) => {
                anyhow::ensure!(
                    radius <= 16 && radius != 8,
                    "圆角范围为 0–16 px，不支持 8 px"
                );
                self.repository
                    .set(CORNER_RADIUS_KEY, &radius.to_string())?;
            }
        }
        Ok(())
    }

    pub fn card_corner(&self) -> Result<CardCornerPreference> {
        Ok(match self.repository.get(CARD_CORNER_KEY)?.as_deref() {
            Some("global") => CardCornerPreference::Global,
            Some("off") => CardCornerPreference::Off,
            _ => CardCornerPreference::Half,
        })
    }

    pub fn set_card_corner(&self, preference: CardCornerPreference) -> Result<()> {
        self.repository.set(
            CARD_CORNER_KEY,
            match preference {
                CardCornerPreference::Half => "half",
                CardCornerPreference::Global => "global",
                CardCornerPreference::Off => "off",
            },
        )?;
        Ok(())
    }
}
