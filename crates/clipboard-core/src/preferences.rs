use crate::database::{Database, SettingsRepository};
use anyhow::Result;

const THEME_KEY: &str = "gpui_theme_mode";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

pub struct Preferences {
    repository: SettingsRepository,
}

impl Preferences {
    pub fn new(db: &Database) -> Self {
        Self {
            repository: SettingsRepository::new(db),
        }
    }

    pub fn theme(&self) -> Result<ThemePreference> {
        Ok(match self.repository.get(THEME_KEY)?.as_deref() {
            Some("light") => ThemePreference::Light,
            Some("dark") => ThemePreference::Dark,
            _ => ThemePreference::System,
        })
    }

    pub fn set_theme(&self, theme: ThemePreference) -> Result<()> {
        Ok(self.repository.set(
            THEME_KEY,
            match theme {
                ThemePreference::System => "system",
                ThemePreference::Light => "light",
                ThemePreference::Dark => "dark",
            },
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_defaults_persists_and_recovers_unknown_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        {
            let db = Database::new(path.clone())?;
            let preferences = Preferences::new(&db);
            assert_eq!(preferences.theme()?, ThemePreference::System);
            for theme in [
                ThemePreference::Light,
                ThemePreference::System,
                ThemePreference::Dark,
            ] {
                preferences.set_theme(theme)?;
                assert_eq!(preferences.theme()?, theme);
            }
        }
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.theme()?, ThemePreference::Dark);
        SettingsRepository::new(&db).set(THEME_KEY, "unknown")?;
        assert_eq!(preferences.theme()?, ThemePreference::System);
        Ok(())
    }
}
