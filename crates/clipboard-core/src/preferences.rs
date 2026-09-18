use crate::database::{Database, SettingsRepository};
use anyhow::Result;

const THEME_KEY: &str = "gpui_theme_mode";
const HOTKEY_KEY: &str = "gpui_hotkey";
const CAPTURE_PAUSED_KEY: &str = "gpui_capture_paused";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HotkeyPreference {
    #[default]
    CtrlShiftV,
    AltC,
    CtrlAltV,
    Disabled,
}

impl HotkeyPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::CtrlShiftV => "Ctrl+Shift+V",
            Self::AltC => "Alt+C",
            Self::CtrlAltV => "Ctrl+Alt+V",
            Self::Disabled => "关闭",
        }
    }
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

    pub fn hotkey(&self) -> Result<HotkeyPreference> {
        Ok(match self.repository.get(HOTKEY_KEY)?.as_deref() {
            Some("alt_c") => HotkeyPreference::AltC,
            Some("ctrl_alt_v") => HotkeyPreference::CtrlAltV,
            Some("disabled") => HotkeyPreference::Disabled,
            _ => HotkeyPreference::CtrlShiftV,
        })
    }

    pub fn set_hotkey(&self, hotkey: HotkeyPreference) -> Result<()> {
        Ok(self.repository.set(
            HOTKEY_KEY,
            match hotkey {
                HotkeyPreference::CtrlShiftV => "ctrl_shift_v",
                HotkeyPreference::AltC => "alt_c",
                HotkeyPreference::CtrlAltV => "ctrl_alt_v",
                HotkeyPreference::Disabled => "disabled",
            },
        )?)
    }

    pub fn capture_paused(&self) -> Result<bool> {
        Ok(!matches!(
            self.repository.get(CAPTURE_PAUSED_KEY)?.as_deref(),
            None | Some("false")
        ))
    }

    pub fn set_capture_paused(&self, paused: bool) -> Result<()> {
        Ok(self
            .repository
            .set(CAPTURE_PAUSED_KEY, if paused { "true" } else { "false" })?)
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

    #[test]
    fn hotkey_preference_persists_independently_of_legacy_setting() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
        SettingsRepository::new(&db).set("global_shortcut", "Alt+C")?;
        assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
        preferences.set_hotkey(HotkeyPreference::CtrlAltV)?;
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlAltV);
        preferences.set_hotkey(HotkeyPreference::Disabled)?;
        assert_eq!(preferences.hotkey()?, HotkeyPreference::Disabled);
        SettingsRepository::new(&db).set(HOTKEY_KEY, "unknown")?;
        assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
        Ok(())
    }

    #[test]
    fn paused_capture_survives_restart_and_unknown_values_stay_paused() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert!(!preferences.capture_paused()?);
        preferences.set_capture_paused(true)?;
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert!(preferences.capture_paused()?);
        preferences.set_capture_paused(false)?;
        assert!(!preferences.capture_paused()?);
        SettingsRepository::new(&db).set(CAPTURE_PAUSED_KEY, "unknown")?;
        assert!(preferences.capture_paused()?);
        Ok(())
    }
}
