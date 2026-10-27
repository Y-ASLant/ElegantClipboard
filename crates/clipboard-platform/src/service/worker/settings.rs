use super::super::capture::lock_capture_state;
use super::{Event, Worker};
use crate::sequence::current_sequence;
use anyhow::{Context, Result};
use clipboard_core::preferences::{
    AppFilterPreference, HotkeyPreference, MonitorTypesPreference, PasteShortcutConfig, Preferences,
};

fn save_hotkey(preferences: &Preferences, hotkey: HotkeyPreference) -> Result<HotkeyPreference> {
    let hotkey = match hotkey {
        HotkeyPreference::Shortcut(value) => {
            HotkeyPreference::Shortcut(crate::hotkey::normalize_main_shortcut(&value)?)
        }
        HotkeyPreference::Disabled => HotkeyPreference::Disabled,
    };
    let shortcuts = preferences.paste_shortcuts()?;
    crate::hotkey::validate_paste_shortcuts(&shortcuts, &hotkey)?;
    crate::hotkey::validate_sequence_shortcut(
        &shortcuts,
        &preferences.sequence_paste_hotkey()?,
        &hotkey,
    )?;
    preferences.set_hotkey(hotkey.clone())?;
    Ok(hotkey)
}

fn save_paste_shortcuts(preferences: &Preferences, shortcuts: &PasteShortcutConfig) -> Result<()> {
    let hotkey = preferences.hotkey()?;
    crate::hotkey::validate_paste_shortcuts(shortcuts, &hotkey)?;
    crate::hotkey::validate_sequence_shortcut(
        shortcuts,
        &preferences.sequence_paste_hotkey()?,
        &hotkey,
    )?;
    preferences.set_paste_shortcuts(shortcuts)
}

fn save_sequence_hotkey(preferences: &Preferences, shortcut: &str) -> Result<String> {
    let shortcut = crate::hotkey::normalize_paste_shortcut(shortcut)?;
    let shortcuts = preferences.paste_shortcuts()?;
    let hotkey = preferences.hotkey()?;
    crate::hotkey::validate_paste_shortcuts(&shortcuts, &hotkey)?;
    crate::hotkey::validate_sequence_shortcut(&shortcuts, &shortcut, &hotkey)?;
    preferences.set_sequence_paste_hotkey(&shortcut)?;
    Ok(shortcut)
}

impl Worker {
    pub(super) fn save_setting<T: Copy>(
        &self,
        value: T,
        save: fn(&Preferences, T) -> Result<()>,
        event: fn(std::result::Result<T, String>) -> Event,
    ) -> Result<()> {
        let result = save(&self.preferences, value)
            .map(|_| value)
            .map_err(|error| error.to_string());
        self.send_event(event(result))
    }

    pub(super) fn handle_set_hotkey(&mut self, hotkey: HotkeyPreference) -> Result<()> {
        let result = save_hotkey(&self.preferences, hotkey).map_err(|error| error.to_string());
        self.send_event(Event::HotkeySaved(result))
    }

    pub(super) fn handle_set_paste_shortcuts(
        &mut self,
        shortcuts: Box<PasteShortcutConfig>,
    ) -> Result<()> {
        let result = save_paste_shortcuts(&self.preferences, &shortcuts)
            .map(|()| shortcuts)
            .map_err(|error| error.to_string());
        self.send_event(Event::PasteShortcutsSaved(result))
    }

    pub(super) fn handle_set_sequence_hotkey(&mut self, shortcut: String) -> Result<()> {
        let result =
            save_sequence_hotkey(&self.preferences, &shortcut).map_err(|error| error.to_string());
        self.send_event(Event::SequenceHotkeySaved(result))
    }

    pub(super) fn handle_set_monitor_types(
        &mut self,
        preference: MonitorTypesPreference,
    ) -> Result<()> {
        let result = self
            .preferences
            .set_monitor_types(preference)
            .map(|_| {
                self.monitor_types = preference;
                preference
            })
            .map_err(|error| error.to_string());
        self.send_event(Event::MonitorTypesSaved(result))
    }

    pub(super) fn handle_set_app_filter(
        &mut self,
        preference: Box<AppFilterPreference>,
    ) -> Result<()> {
        let result = self
            .preferences
            .set_app_filter(&preference)
            .map(|_| {
                self.app_filter = (*preference).clone();
                preference
            })
            .map_err(|error| error.to_string());
        self.send_event(Event::AppFilterSaved(result))
    }

    pub(super) fn handle_set_transient_filter(&mut self, enabled: bool) -> Result<()> {
        let result = self
            .preferences
            .set_transient_filter(enabled)
            .map(|_| {
                self.transient.set_enabled(enabled);
                enabled
            })
            .map_err(|error| error.to_string());
        self.send_event(Event::TransientFilterSaved(result))
    }

    pub(super) fn handle_pause(&mut self, paused: bool) -> Result<()> {
        self.preferences
            .set_capture_paused(paused)
            .context("无法保存暂停状态")?;
        let mut state = lock_capture_state(&self.state)?;
        state.paused = paused;
        state.last_sequence = current_sequence(&self.clipboard)?;
        drop(state);
        self.send_event(Event::Paused(paused))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipboard_core::database::Database;

    #[test]
    fn conflicting_changes_leave_all_persisted_shortcuts_intact() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("settings.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        let main = HotkeyPreference::Shortcut("Ctrl+Alt+F7".into());
        save_hotkey(&preferences, main.clone())?;
        let sequence = save_sequence_hotkey(&preferences, " alt + control + f8 ")?;
        let slots = preferences.paste_shortcuts()?;
        assert!(save_hotkey(&preferences, HotkeyPreference::Shortcut(sequence.clone())).is_err());
        let mut conflicting = slots.clone();
        conflicting.set_slot(false, 1, sequence.clone())?;
        assert!(save_paste_shortcuts(&preferences, &conflicting).is_err());
        assert!(save_sequence_hotkey(&preferences, "Ctrl+Alt+F7").is_err());
        assert!(save_sequence_hotkey(&preferences, slots.slot(false, 1).unwrap()).is_err());
        assert!(save_sequence_hotkey(&preferences, "Ctrl+").is_err());
        let reopened = Database::new(path)?;
        let persisted = Preferences::new(&reopened);
        assert_eq!(persisted.hotkey()?, main);
        assert_eq!(persisted.paste_shortcuts()?, slots);
        assert_eq!(persisted.sequence_paste_hotkey()?, sequence);
        assert_eq!(sequence, "Ctrl+Alt+F8");
        save_sequence_hotkey(&preferences, "")?;
        save_hotkey(&preferences, HotkeyPreference::Shortcut(sequence.clone()))?;
        assert_eq!(preferences.hotkey()?, HotkeyPreference::Shortcut(sequence));
        Ok(())
    }

    #[test]
    fn storage_errors_do_not_mutate_shortcuts_and_allow_retry() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("settings.db"))?;
        let preferences = Preferences::new(&db);
        let main = preferences.hotkey()?;
        let sequence = preferences.sequence_paste_hotkey()?;
        let slots = preferences.paste_shortcuts()?;
        let mut next_slots = slots.clone();
        next_slots.set_slot(false, 1, "Ctrl+Alt+F9".into())?;
        db.write_connection()
            .lock()
            .execute_batch("PRAGMA query_only = ON;")?;
        assert!(
            save_hotkey(
                &preferences,
                HotkeyPreference::Shortcut("Ctrl+Alt+F7".into())
            )
            .is_err()
        );
        assert!(save_paste_shortcuts(&preferences, &next_slots).is_err());
        assert!(save_sequence_hotkey(&preferences, "Ctrl+Alt+F8").is_err());
        assert_eq!(preferences.hotkey()?, main);
        assert_eq!(preferences.paste_shortcuts()?, slots);
        assert_eq!(preferences.sequence_paste_hotkey()?, sequence);
        db.write_connection()
            .lock()
            .execute_batch("PRAGMA query_only = OFF;")?;
        save_hotkey(
            &preferences,
            HotkeyPreference::Shortcut("Ctrl+Alt+F7".into()),
        )?;
        save_paste_shortcuts(&preferences, &next_slots)?;
        save_sequence_hotkey(&preferences, "Ctrl+Alt+F8")?;
        assert_eq!(
            preferences.hotkey()?,
            HotkeyPreference::Shortcut("Ctrl+Alt+F7".into())
        );
        assert_eq!(preferences.paste_shortcuts()?, next_slots);
        assert_eq!(preferences.sequence_paste_hotkey()?, "Ctrl+Alt+F8");
        Ok(())
    }
}
