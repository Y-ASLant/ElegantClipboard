use crate::database::{Database, SettingsRepository};
use anyhow::Result;

const THEME_KEY: &str = "gpui_theme_mode";
const HOTKEY_KEY: &str = "gpui_hotkey";
const CAPTURE_PAUSED_KEY: &str = "gpui_capture_paused";
const WINDOW_SIZE_KEY: &str = "gpui_window_size";
pub(crate) const PRUNE_NON_GPUI_SETTINGS_SQL: &str = "DELETE FROM settings WHERE key NOT IN
     ('gpui_theme_mode', 'gpui_hotkey', 'gpui_capture_paused', 'gpui_window_size')";

pub const MIN_WINDOW_WIDTH: u32 = 420;
pub const MIN_WINDOW_HEIGHT: u32 = 520;
pub const MAX_WINDOW_WIDTH: u32 = 4096;
pub const MAX_WINDOW_HEIGHT: u32 = 2160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSizePreference {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowSizePreference {
    fn default() -> Self {
        Self {
            width: 560,
            height: 760,
        }
    }
}

impl WindowSizePreference {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        (MIN_WINDOW_WIDTH..=MAX_WINDOW_WIDTH)
            .contains(&width)
            .then_some(())?;
        (MIN_WINDOW_HEIGHT..=MAX_WINDOW_HEIGHT)
            .contains(&height)
            .then_some(Self { width, height })
    }

    fn parse(value: &str) -> Option<Self> {
        let (width, height) = value.split_once('x')?;
        Self::new(width.parse().ok()?, height.parse().ok()?)
    }
}

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

    pub fn window_size(&self) -> Result<Option<WindowSizePreference>> {
        Ok(self
            .repository
            .get(WINDOW_SIZE_KEY)?
            .as_deref()
            .and_then(WindowSizePreference::parse))
    }

    pub fn set_window_size(&self, size: WindowSizePreference) -> Result<()> {
        Ok(self
            .repository
            .set(WINDOW_SIZE_KEY, &format!("{}x{}", size.width, size.height))?)
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

    #[test]
    fn window_size_persists_and_invalid_values_fall_back() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.window_size()?, None);
        let size = WindowSizePreference::new(900, 700).unwrap();
        preferences.set_window_size(size)?;
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.window_size()?, Some(size));
        for invalid in ["419x700", "900x519", "4097x700", "900x2161", "broken"] {
            SettingsRepository::new(&db).set(WINDOW_SIZE_KEY, invalid)?;
            assert_eq!(preferences.window_size()?, None);
        }
        Ok(())
    }
}
