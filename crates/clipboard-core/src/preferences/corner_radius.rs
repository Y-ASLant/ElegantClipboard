use super::{CORNER_RADIUS_KEY, CornerRadiusPreference, Preferences};
use anyhow::Result;

impl Preferences {
    pub fn corner_radius(&self) -> Result<CornerRadiusPreference> {
        let stored = self.repository.get(CORNER_RADIUS_KEY)?;
        if stored.as_deref() == Some("system") {
            return Ok(CornerRadiusPreference::System);
        }
        match stored.and_then(|value| value.parse::<u8>().ok()) {
            Some(8) => {
                self.repository.set(CORNER_RADIUS_KEY, "system")?;
                Ok(CornerRadiusPreference::System)
            }
            Some(radius @ 0..=16) => Ok(CornerRadiusPreference::Fixed(radius)),
            _ => Ok(CornerRadiusPreference::default()),
        }
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
}
