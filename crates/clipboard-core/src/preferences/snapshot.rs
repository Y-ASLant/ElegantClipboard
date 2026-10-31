use super::*;
use crate::retention::RetentionPolicy;

/// Typed startup settings, using the same validation and defaults as live preference reads.
#[derive(Debug, PartialEq, Eq)]
pub struct PreferenceSnapshot {
    pub theme: ThemePreference,
    pub corner_radius: CornerRadiusPreference,
    pub language: LanguagePreference,
    pub hotkey: HotkeyPreference,
    pub paused: bool,
    pub window_size: Option<WindowSizePreference>,
    pub persist_window_size: bool,
    pub auto_reset_state: bool,
    pub search_auto_focus: bool,
    pub search_auto_clear: bool,
    pub skip_clear_confirm: bool,
    pub paste_close_window: bool,
    pub paste_key: PasteKeyPreference,
    pub paste_move_to_top: bool,
    pub dedup_strategy: DedupStrategy,
    pub restore_clipboard: bool,
    pub sequence_hotkey: String,
    pub quick_paste_enabled: bool,
    pub paste_shortcuts: PasteShortcutConfig,
    pub window_position: WindowPositionPreference,
    pub hover_preview: HoverPreviewPreference,
    pub display: DisplayPreference,
    pub audio: AudioPreference,
    pub retention: RetentionPolicy,
    pub retention_saved: bool,
    pub transient_filter: bool,
    pub monitor_types: MonitorTypesPreference,
    pub app_filter: AppFilterPreference,
    pub window_effect: WindowEffectPreference,
    pub onboarding_completed: bool,
}

impl Preferences {
    pub fn snapshot(&self) -> Result<PreferenceSnapshot> {
        Ok(PreferenceSnapshot {
            theme: self.theme()?,
            corner_radius: self.corner_radius()?,
            language: self.language()?,
            hotkey: self.hotkey()?,
            paused: self.capture_paused()?,
            window_size: self.window_size()?,
            persist_window_size: self.persist_window_size()?,
            auto_reset_state: self.auto_reset_state()?,
            search_auto_focus: self.search_auto_focus()?,
            search_auto_clear: self.search_auto_clear()?,
            skip_clear_confirm: self.skip_clear_confirm()?,
            paste_close_window: self.paste_close_window()?,
            paste_key: self.paste_key()?,
            paste_move_to_top: self.paste_move_to_top()?,
            dedup_strategy: self.dedup_strategy()?,
            restore_clipboard: self.restore_clipboard()?,
            sequence_hotkey: self.sequence_paste_hotkey()?,
            quick_paste_enabled: self.quick_paste_enabled()?,
            paste_shortcuts: self.paste_shortcuts()?,
            window_position: self.window_position()?,
            hover_preview: self.hover_preview()?,
            display: self.display()?,
            audio: self.audio()?,
            retention: self.retention()?,
            retention_saved: self.retention_saved()?,
            transient_filter: self.transient_filter()?,
            monitor_types: self.monitor_types()?,
            app_filter: self.app_filter()?,
            window_effect: self.window_effect()?,
            onboarding_completed: self.onboarding_completed()?,
        })
    }
}
