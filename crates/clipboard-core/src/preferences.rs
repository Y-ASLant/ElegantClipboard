use crate::database::{Database, SettingsRepository};
use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};

mod backup;
mod corner_radius;
mod hover_preview;
mod snapshot;
mod types;
mod window_effect;
pub(crate) use backup::prune_non_gpui_settings;
pub use hover_preview::*;
pub use snapshot::PreferenceSnapshot;
pub use types::*;

pub(crate) const RETENTION_KEY: &str = "gpui_retention";
const THEME_KEY: &str = "gpui_theme_mode";
const CORNER_RADIUS_KEY: &str = "gpui_corner_radius";
const LANGUAGE_KEY: &str = "gpui_language";
const HOTKEY_KEY: &str = "gpui_hotkey";
const CAPTURE_PAUSED_KEY: &str = "gpui_capture_paused";
const TRANSIENT_FILTER_KEY: &str = "gpui_transient_filter";
const WINDOW_SIZE_KEY: &str = "gpui_window_size";
const PERSIST_WINDOW_SIZE_KEY: &str = "gpui_persist_window_size";
const AUTO_RESET_STATE_KEY: &str = "gpui_auto_reset_state";
const SEARCH_AUTO_FOCUS_KEY: &str = "gpui_search_auto_focus";
const SEARCH_AUTO_CLEAR_KEY: &str = "gpui_search_auto_clear";
const SKIP_CLEAR_CONFIRM_KEY: &str = "gpui_skip_clear_confirm";
const PASTE_CLOSE_WINDOW_KEY: &str = "gpui_paste_close_window";
const PASTE_KEY_KEY: &str = "gpui_paste_key";
const PASTE_MOVE_TO_TOP_KEY: &str = "gpui_paste_move_to_top";
const QUICK_PASTE_ENABLED_KEY: &str = "gpui_quick_paste_enabled";
const PASTE_SHORTCUTS_KEY: &str = "gpui_paste_shortcuts";
const WINDOW_POSITION_KEY: &str = "gpui_window_position";
const HOVER_PREVIEW_KEY: &str = "gpui_hover_preview";
const DISPLAY_KEY: &str = "gpui_display";
const MONITOR_TYPES_KEY: &str = "gpui_monitor_types";
const APP_FILTER_KEY: &str = "gpui_app_filter";
const ONBOARDING_COMPLETED_KEY: &str = "gpui_onboarding_completed";
const AUDIO_KEY: &str = "gpui_audio";
const WINDOW_EFFECT_KEY: &str = "gpui_window_effect";
const DEDUP_STRATEGY_KEY: &str = "gpui_dedup_strategy";
const RESTORE_CLIPBOARD_KEY: &str = "gpui_restore_clipboard";
const SEQUENCE_HOTKEY_KEY: &str = "gpui_sequence_paste_hotkey";

pub struct Preferences {
    repository: SettingsRepository,
}

impl Preferences {
    pub fn new(db: &Database) -> Self {
        Self {
            repository: SettingsRepository::new(db),
        }
    }

    /// 读取 bool 偏好；存储值不是 "true"/"false"（或缺失）时取 `default`。
    fn bool_pref(&self, key: &str, default: bool) -> Result<bool> {
        Ok(self
            .repository
            .get(key)?
            .as_deref()
            .and_then(|value| match value {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            })
            .unwrap_or(default))
    }

    fn set_bool_pref(&self, key: &str, enabled: bool) -> Result<()> {
        Ok(self
            .repository
            .set(key, if enabled { "true" } else { "false" })?)
    }

    pub fn retention_saved(&self) -> Result<bool> {
        Ok(self.repository.get(RETENTION_KEY)?.is_some())
    }

    pub fn retention(&self) -> Result<crate::retention::RetentionPolicy> {
        match self.repository.get(RETENTION_KEY)? {
            Some(value) => Ok(serde_json::from_str(&value)?),
            None => Ok(crate::retention::RetentionPolicy::default()),
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

    pub fn language(&self) -> Result<LanguagePreference> {
        Ok(match self.repository.get(LANGUAGE_KEY)?.as_deref() {
            Some("en") => LanguagePreference::English,
            _ => LanguagePreference::Chinese,
        })
    }

    pub fn set_language(&self, language: LanguagePreference) -> Result<()> {
        Ok(self.repository.set(
            LANGUAGE_KEY,
            match language {
                LanguagePreference::Chinese => "zh-CN",
                LanguagePreference::English => "en",
            },
        )?)
    }

    pub fn hotkey(&self) -> Result<HotkeyPreference> {
        Ok(match self.repository.get(HOTKEY_KEY)?.as_deref() {
            Some("ctrl_shift_v") => HotkeyPreference::Shortcut("Ctrl+Shift+V".into()),
            Some("alt_c") => HotkeyPreference::Shortcut("Alt+C".into()),
            Some("ctrl_alt_v") => HotkeyPreference::Shortcut("Ctrl+Alt+V".into()),
            Some("disabled") => HotkeyPreference::Disabled,
            Some(value) if value.contains('+') => HotkeyPreference::Shortcut(value.to_owned()),
            _ => HotkeyPreference::default(),
        })
    }

    pub fn set_hotkey(&self, hotkey: HotkeyPreference) -> Result<()> {
        Ok(self.repository.set(
            HOTKEY_KEY,
            match &hotkey {
                HotkeyPreference::Shortcut(shortcut) => shortcut,
                HotkeyPreference::Disabled => "disabled",
            },
        )?)
    }

    /// 瞬时内容确认窗口开关；缺省开启（旧行为的增强，减少划词/碎片噪声）。
    pub fn transient_filter(&self) -> Result<bool> {
        self.bool_pref(TRANSIENT_FILTER_KEY, true)
    }

    pub fn set_transient_filter(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(TRANSIENT_FILTER_KEY, enabled)
    }

    /// 暂停状态特殊：`"false"` 或缺失值表示采集中，其余（含未知值）视为已暂停
    /// （保守行为，防止迁移残留意外恢复采集）。
    pub fn capture_paused(&self) -> Result<bool> {
        Ok(!matches!(
            self.repository.get(CAPTURE_PAUSED_KEY)?.as_deref(),
            None | Some("false")
        ))
    }

    pub fn set_capture_paused(&self, paused: bool) -> Result<()> {
        self.set_bool_pref(CAPTURE_PAUSED_KEY, paused)
    }

    pub fn window_size(&self) -> Result<Option<WindowSizePreference>> {
        if !self.persist_window_size()? {
            return Ok(None);
        }
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

    pub fn persist_window_size(&self) -> Result<bool> {
        self.bool_pref(PERSIST_WINDOW_SIZE_KEY, true)
    }

    pub fn set_persist_window_size(&self, enabled: bool) -> Result<()> {
        if !enabled {
            self.repository
                .set_batch(&[(WINDOW_SIZE_KEY, ""), (PERSIST_WINDOW_SIZE_KEY, "false")])?;
        } else {
            self.repository.set(PERSIST_WINDOW_SIZE_KEY, "true")?;
        }
        Ok(())
    }

    pub fn auto_reset_state(&self) -> Result<bool> {
        self.bool_pref(AUTO_RESET_STATE_KEY, false)
    }

    pub fn set_auto_reset_state(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(AUTO_RESET_STATE_KEY, enabled)
    }

    pub fn search_auto_focus(&self) -> Result<bool> {
        self.bool_pref(SEARCH_AUTO_FOCUS_KEY, false)
    }

    pub fn set_search_auto_focus(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(SEARCH_AUTO_FOCUS_KEY, enabled)
    }

    pub fn search_auto_clear(&self) -> Result<bool> {
        self.bool_pref(SEARCH_AUTO_CLEAR_KEY, true)
    }

    pub fn set_search_auto_clear(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(SEARCH_AUTO_CLEAR_KEY, enabled)
    }

    pub fn skip_clear_confirm(&self) -> Result<bool> {
        self.bool_pref(SKIP_CLEAR_CONFIRM_KEY, false)
    }

    pub fn set_skip_clear_confirm(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(SKIP_CLEAR_CONFIRM_KEY, enabled)
    }

    pub fn paste_close_window(&self) -> Result<bool> {
        self.bool_pref(PASTE_CLOSE_WINDOW_KEY, true)
    }

    pub fn set_paste_close_window(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(PASTE_CLOSE_WINDOW_KEY, enabled)
    }

    pub fn paste_key(&self) -> Result<PasteKeyPreference> {
        Ok(match self.repository.get(PASTE_KEY_KEY)?.as_deref() {
            Some("shift_insert") => PasteKeyPreference::ShiftInsert,
            _ => PasteKeyPreference::CtrlV,
        })
    }

    pub fn set_paste_key(&self, key: PasteKeyPreference) -> Result<()> {
        self.repository.set(
            PASTE_KEY_KEY,
            match key {
                PasteKeyPreference::CtrlV => "ctrl_v",
                PasteKeyPreference::ShiftInsert => "shift_insert",
            },
        )?;
        Ok(())
    }

    pub fn paste_move_to_top(&self) -> Result<bool> {
        self.bool_pref(PASTE_MOVE_TO_TOP_KEY, true)
    }

    pub fn set_paste_move_to_top(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(PASTE_MOVE_TO_TOP_KEY, enabled)
    }

    pub fn quick_paste_enabled(&self) -> Result<bool> {
        self.bool_pref(QUICK_PASTE_ENABLED_KEY, false)
    }

    pub fn set_quick_paste_enabled(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(QUICK_PASTE_ENABLED_KEY, enabled)
    }

    fn read_json<T: DeserializeOwned + Default>(
        &self,
        key: &str,
        valid: impl FnOnce(&T) -> bool,
    ) -> Result<T> {
        Ok(self
            .repository
            .get(key)?
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .filter(valid)
            .unwrap_or_default())
    }

    fn write_json<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        self.repository.set(key, &serde_json::to_string(value)?)?;
        Ok(())
    }

    pub fn paste_shortcuts(&self) -> Result<PasteShortcutConfig> {
        self.read_json(PASTE_SHORTCUTS_KEY, |_| true)
    }

    pub fn set_paste_shortcuts(&self, shortcuts: &PasteShortcutConfig) -> Result<()> {
        self.write_json(PASTE_SHORTCUTS_KEY, shortcuts)
    }

    pub fn window_position(&self) -> Result<WindowPositionPreference> {
        Ok(match self.repository.get(WINDOW_POSITION_KEY)?.as_deref() {
            Some("screen_center") => WindowPositionPreference::ScreenCenter,
            Some("fixed_position") => WindowPositionPreference::FixedPosition,
            _ => WindowPositionPreference::FollowCursor,
        })
    }

    pub fn set_window_position(&self, position: WindowPositionPreference) -> Result<()> {
        self.repository.set(
            WINDOW_POSITION_KEY,
            match position {
                WindowPositionPreference::FollowCursor => "follow_cursor",
                WindowPositionPreference::ScreenCenter => "screen_center",
                WindowPositionPreference::FixedPosition => "fixed_position",
            },
        )?;
        Ok(())
    }

    pub fn hover_preview(&self) -> Result<HoverPreviewPreference> {
        self.read_json(HOVER_PREVIEW_KEY, |preference: &HoverPreviewPreference| {
            preference.valid()
        })
    }

    pub fn set_hover_preview(&self, preference: HoverPreviewPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "悬停预览设置超出允许范围");
        self.write_json(HOVER_PREVIEW_KEY, &preference)
    }

    pub fn display(&self) -> Result<DisplayPreference> {
        self.read_json(DISPLAY_KEY, |preference: &DisplayPreference| {
            preference.valid()
        })
    }

    pub fn set_display(&self, preference: DisplayPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "显示设置超出允许范围");
        self.write_json(DISPLAY_KEY, &preference)
    }

    pub fn audio(&self) -> Result<AudioPreference> {
        self.read_json(AUDIO_KEY, |_| true)
    }

    pub fn set_audio(&self, preference: AudioPreference) -> Result<()> {
        self.write_json(AUDIO_KEY, &preference)
    }

    pub fn monitor_types(&self) -> Result<MonitorTypesPreference> {
        self.read_json(MONITOR_TYPES_KEY, |preference: &MonitorTypesPreference| {
            preference.valid()
        })
    }

    pub fn set_monitor_types(&self, preference: MonitorTypesPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "至少需要监听一种内容类型");
        self.write_json(MONITOR_TYPES_KEY, &preference)
    }

    pub fn app_filter(&self) -> Result<AppFilterPreference> {
        self.read_json(APP_FILTER_KEY, AppFilterPreference::valid)
    }

    /// 去重策略：move_to_top（默认，同哈希移顶）或 always_new（同哈希也新建）。
    pub fn dedup_strategy(&self) -> Result<DedupStrategy> {
        Ok(match self.repository.get(DEDUP_STRATEGY_KEY)?.as_deref() {
            Some("always_new") => DedupStrategy::AlwaysNew,
            _ => DedupStrategy::MoveToTop,
        })
    }

    pub fn set_dedup_strategy(&self, strategy: DedupStrategy) -> Result<()> {
        let value = match strategy {
            DedupStrategy::MoveToTop => "move_to_top",
            DedupStrategy::AlwaysNew => "always_new",
        };
        Ok(self.repository.set(DEDUP_STRATEGY_KEY, value)?)
    }

    /// 受控粘贴后是否把用户原剪贴板内容写回（见决策 0003，默认关闭）。
    pub fn restore_clipboard(&self) -> Result<bool> {
        self.bool_pref(RESTORE_CLIPBOARD_KEY, false)
    }

    pub fn set_restore_clipboard(&self, enabled: bool) -> Result<()> {
        self.set_bool_pref(RESTORE_CLIPBOARD_KEY, enabled)
    }

    /// 序列粘贴推进键（决策 0004）；空串表示停用。默认 Ctrl+Shift+G。
    pub fn sequence_paste_hotkey(&self) -> Result<String> {
        Ok(self
            .repository
            .get(SEQUENCE_HOTKEY_KEY)?
            .unwrap_or_else(|| "Ctrl+Shift+G".into()))
    }

    pub fn set_sequence_paste_hotkey(&self, shortcut: &str) -> Result<()> {
        Ok(self.repository.set(SEQUENCE_HOTKEY_KEY, shortcut.trim())?)
    }

    pub fn set_app_filter(&self, preference: &AppFilterPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "应用过滤规则无效");
        self.write_json(APP_FILTER_KEY, preference)
    }

    pub fn onboarding_completed(&self) -> Result<bool> {
        self.bool_pref(ONBOARDING_COMPLETED_KEY, false)
    }

    pub fn set_onboarding_completed(&self) -> Result<()> {
        self.repository.set(ONBOARDING_COMPLETED_KEY, "true")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_content;
#[cfg(test)]
mod tests_corner_radius;
#[cfg(test)]
mod tests_window_effect;
