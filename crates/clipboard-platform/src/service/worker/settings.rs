use super::super::capture::lock_capture_state;
use super::{Event, Worker};
use crate::sequence::current_sequence;
use anyhow::{Context, Result};
use clipboard_core::preferences::{
    AppFilterPreference, HotkeyPreference, MonitorTypesPreference, PasteShortcutConfig, Preferences,
};

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
        let result = (|| {
            let hotkey = match hotkey {
                HotkeyPreference::Shortcut(value) => {
                    let normalized = crate::hotkey::normalize_main_shortcut(&value)?;
                    HotkeyPreference::Shortcut(normalized)
                }
                HotkeyPreference::Disabled => HotkeyPreference::Disabled,
            };
            crate::hotkey::validate_paste_shortcuts(&self.preferences.paste_shortcuts()?, &hotkey)?;
            self.preferences.set_hotkey(hotkey.clone())?;
            Ok::<_, anyhow::Error>(hotkey)
        })()
        .map_err(|error| error.to_string());
        self.send_event(Event::HotkeySaved(result))
    }

    pub(super) fn handle_set_paste_shortcuts(
        &mut self,
        shortcuts: Box<PasteShortcutConfig>,
    ) -> Result<()> {
        let result =
            crate::hotkey::validate_paste_shortcuts(&shortcuts, &self.preferences.hotkey()?)
                .and_then(|_| self.preferences.set_paste_shortcuts(&shortcuts))
                .map(|_| shortcuts)
                .map_err(|error| error.to_string());
        self.send_event(Event::PasteShortcutsSaved(result))
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
