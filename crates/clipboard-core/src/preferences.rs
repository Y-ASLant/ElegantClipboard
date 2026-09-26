use crate::database::{Database, SettingsRepository};
use anyhow::Result;
use serde::{Deserialize, Serialize};

const THEME_KEY: &str = "gpui_theme_mode";
const LANGUAGE_KEY: &str = "gpui_language";
const HOTKEY_KEY: &str = "gpui_hotkey";
const CAPTURE_PAUSED_KEY: &str = "gpui_capture_paused";
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
const TOOLBAR_KEY: &str = "gpui_toolbar";
const DISPLAY_KEY: &str = "gpui_display";
const MONITOR_TYPES_KEY: &str = "gpui_monitor_types";
const APP_FILTER_KEY: &str = "gpui_app_filter";
const ONBOARDING_COMPLETED_KEY: &str = "gpui_onboarding_completed";
const AUDIO_KEY: &str = "gpui_audio";
pub(crate) const PRUNE_NON_GPUI_SETTINGS_SQL: &str = "DELETE FROM settings WHERE key NOT IN
     ('gpui_theme_mode', 'gpui_language', 'gpui_hotkey', 'gpui_capture_paused', 'gpui_window_size',
      'gpui_hover_preview', 'gpui_window_position', 'gpui_persist_window_size',
      'gpui_auto_reset_state', 'gpui_search_auto_focus', 'gpui_search_auto_clear', 'gpui_skip_clear_confirm',
      'gpui_paste_close_window', 'gpui_paste_key', 'gpui_paste_move_to_top', 'gpui_quick_paste_enabled', 'gpui_paste_shortcuts',
      'gpui_toolbar', 'gpui_display', 'gpui_monitor_types', 'gpui_app_filter',
      'gpui_onboarding_completed', 'gpui_audio')";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundTiming {
    #[default]
    Immediate,
    AfterSuccess,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioPreference {
    pub copy_enabled: bool,
    pub copy_timing: SoundTiming,
    pub paste_enabled: bool,
    pub paste_timing: SoundTiming,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppFilterMode {
    #[default]
    Blacklist,
    Whitelist,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppFilterPreference {
    pub enabled: bool,
    pub mode: AppFilterMode,
    pub rules: Vec<String>,
}

impl AppFilterPreference {
    pub fn valid(&self) -> bool {
        self.rules.len() <= 100
            && self.rules.iter().all(|rule| {
                !rule.is_empty()
                    && rule.len() <= 260
                    && rule.trim() == rule
                    && !rule.chars().any(char::is_control)
            })
            && self.rules.iter().enumerate().all(|(index, rule)| {
                !self.rules[..index]
                    .iter()
                    .any(|previous| previous.eq_ignore_ascii_case(rule))
            })
    }

    pub fn with_rule(mut self, rule: &str) -> Option<Self> {
        let rule = rule.trim();
        if rule.is_empty() {
            return None;
        }
        if self
            .rules
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(rule))
        {
            return Some(self);
        }
        self.rules.push(rule.to_owned());
        self.valid().then_some(self)
    }

    pub fn without_rule(mut self, rule: &str) -> Self {
        self.rules
            .retain(|existing| !existing.eq_ignore_ascii_case(rule));
        self
    }

    pub fn excludes(&self, source_name: Option<&str>, executable: Option<&str>) -> bool {
        if !self.enabled || self.rules.is_empty() || (source_name.is_none() && executable.is_none())
        {
            return false;
        }
        let process_name = executable
            .and_then(|path| path.rsplit(['\\', '/']).next())
            .unwrap_or_default();
        let matches = self.rules.iter().any(|rule| {
            [
                source_name.unwrap_or_default(),
                process_name,
                executable.unwrap_or_default(),
            ]
            .into_iter()
            .any(|candidate| match_rule(rule, candidate))
        });
        match self.mode {
            AppFilterMode::Blacklist => matches,
            AppFilterMode::Whitelist => !matches,
        }
    }
}

fn match_rule(rule: &str, candidate: &str) -> bool {
    if rule.contains(['*', '?']) {
        wildcard_match(rule, candidate)
    } else {
        candidate.to_lowercase().contains(&rule.to_lowercase())
    }
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut p, mut t, mut star, mut retry) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            retry = t;
            p += 1;
        } else if let Some(index) = star {
            retry += 1;
            t = retry;
            p = index + 1;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|ch| *ch == '*')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorTypesPreference {
    pub text: bool,
    pub url: bool,
    pub html: bool,
    pub rtf: bool,
    pub image: bool,
    pub files: bool,
}

impl Default for MonitorTypesPreference {
    fn default() -> Self {
        Self {
            text: true,
            url: true,
            html: true,
            rtf: true,
            image: true,
            files: true,
        }
    }
}

impl MonitorTypesPreference {
    pub fn valid(self) -> bool {
        self.text || self.url || self.html || self.rtf || self.image || self.files
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasteShortcutConfig {
    pub recent: [String; 10],
    pub favorites: [String; 10],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasteKeyPreference {
    #[default]
    CtrlV,
    ShiftInsert,
}

impl Default for PasteShortcutConfig {
    fn default() -> Self {
        Self {
            recent: std::array::from_fn(|index| {
                format!("Alt+{}", if index == 9 { 0 } else { index + 1 })
            }),
            favorites: std::array::from_fn(|index| {
                if index < 3 {
                    format!("Ctrl+Alt+{}", index + 1)
                } else {
                    String::new()
                }
            }),
        }
    }
}

impl PasteShortcutConfig {
    pub fn slot(&self, favorite: bool, slot: u8) -> Option<&str> {
        let index = usize::from(slot.checked_sub(1)?);
        if favorite {
            self.favorites.get(index).map(String::as_str)
        } else {
            self.recent.get(index).map(String::as_str)
        }
    }

    pub fn set_slot(&mut self, favorite: bool, slot: u8, shortcut: String) -> anyhow::Result<()> {
        let index = usize::from(
            slot.checked_sub(1)
                .ok_or_else(|| anyhow::anyhow!("槽位必须在 1 到 10 之间"))?,
        );
        let target = if favorite {
            self.favorites.get_mut(index)
        } else {
            self.recent.get_mut(index)
        };
        *target.ok_or_else(|| anyhow::anyhow!("槽位必须在 1 到 10 之间"))? = shortcut;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardDensity {
    Compact,
    #[default]
    Standard,
    Spacious,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    #[default]
    Absolute,
    Relative,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAppDisplay {
    #[default]
    Both,
    Name,
    Icon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayPreference {
    pub show_category_filter: bool,
    pub show_drag_area_indicator: bool,
    pub card_density: CardDensity,
    pub card_max_lines: u8,
    pub show_time: bool,
    pub time_format: TimeFormat,
    pub show_char_count: bool,
    pub show_byte_size: bool,
    pub show_source_app: bool,
    pub source_app_display: SourceAppDisplay,
}

impl Default for DisplayPreference {
    fn default() -> Self {
        Self {
            show_category_filter: true,
            show_drag_area_indicator: true,
            card_density: CardDensity::Standard,
            card_max_lines: 3,
            show_time: true,
            time_format: TimeFormat::Absolute,
            show_char_count: true,
            show_byte_size: true,
            show_source_app: true,
            source_app_display: SourceAppDisplay::Both,
        }
    }
}

impl DisplayPreference {
    pub fn valid(self) -> bool {
        (1..=10).contains(&self.card_max_lines)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarButton {
    Clear,
    Batch,
    Pin,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolbarItem {
    pub button: ToolbarButton,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolbarPreference {
    pub items: [ToolbarItem; 4],
}

impl Default for ToolbarPreference {
    fn default() -> Self {
        Self {
            items: [
                ToolbarItem {
                    button: ToolbarButton::Clear,
                    visible: true,
                },
                ToolbarItem {
                    button: ToolbarButton::Batch,
                    visible: true,
                },
                ToolbarItem {
                    button: ToolbarButton::Pin,
                    visible: true,
                },
                ToolbarItem {
                    button: ToolbarButton::Settings,
                    visible: true,
                },
            ],
        }
    }
}

impl ToolbarPreference {
    pub fn valid(self) -> bool {
        let mut seen = [false; 4];
        for item in self.items {
            let index = match item.button {
                ToolbarButton::Clear => 0,
                ToolbarButton::Batch => 1,
                ToolbarButton::Pin => 2,
                ToolbarButton::Settings => 3,
            };
            if seen[index] || (index == 3 && !item.visible) {
                return false;
            }
            seen[index] = true;
        }
        seen.into_iter().all(|present| present)
    }

    pub fn with_visibility(mut self, button: ToolbarButton, visible: bool) -> Option<Self> {
        let item = self.items.iter_mut().find(|item| item.button == button)?;
        item.visible = visible;
        self.valid().then_some(self)
    }

    pub fn move_button(mut self, button: ToolbarButton, direction: isize) -> Option<Self> {
        let index = self.items.iter().position(|item| item.button == button)?;
        let next = index.checked_add_signed(direction)?;
        if next >= self.items.len() || next == index {
            return None;
        }
        self.items.swap(index, next);
        Some(self)
    }

    pub fn move_before_or_after(
        mut self,
        button: ToolbarButton,
        target: ToolbarButton,
        after: bool,
    ) -> Option<Self> {
        let from = self.items.iter().position(|item| item.button == button)?;
        let to = self.items.iter().position(|item| item.button == target)?;
        if from == to {
            return None;
        }
        let destination = (to + usize::from(after)).saturating_sub(usize::from(from < to));
        if from == destination {
            return None;
        }
        let moved = self.items[from];
        if from < destination {
            self.items.copy_within(from + 1..=destination, from);
        } else {
            self.items.copy_within(destination..from, destination + 1);
        }
        self.items[destination] = moved;
        Some(self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowPositionPreference {
    #[default]
    FollowCursor,
    ScreenCenter,
    FixedPosition,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoverPreviewPosition {
    #[default]
    Auto,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverPreviewPreference {
    pub image: bool,
    pub text: bool,
    pub files: bool,
    pub expanded_image: bool,
    pub delay_ms: u16,
    pub position: HoverPreviewPosition,
    pub zoom_step: u8,
}

impl Default for HoverPreviewPreference {
    fn default() -> Self {
        Self {
            image: true,
            text: false,
            files: false,
            expanded_image: false,
            delay_ms: 500,
            position: HoverPreviewPosition::Auto,
            zoom_step: 10,
        }
    }
}

impl HoverPreviewPreference {
    pub fn allows(self, content_type: &str) -> bool {
        match content_type {
            "image" => self.image,
            "files" => self.files,
            "text" | "url" | "html" | "rtf" => self.text,
            _ => false,
        }
    }

    fn valid(self) -> bool {
        (100..=2000).contains(&self.delay_ms) && (5..=50).contains(&self.zoom_step)
    }
}

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
pub enum LanguagePreference {
    #[default]
    Chinese,
    English,
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
        Ok(!matches!(
            self.repository.get(PERSIST_WINDOW_SIZE_KEY)?.as_deref(),
            Some("false")
        ))
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
        Ok(matches!(
            self.repository.get(AUTO_RESET_STATE_KEY)?.as_deref(),
            Some("true")
        ))
    }

    pub fn set_auto_reset_state(&self, enabled: bool) -> Result<()> {
        self.repository
            .set(AUTO_RESET_STATE_KEY, if enabled { "true" } else { "false" })?;
        Ok(())
    }

    pub fn search_auto_focus(&self) -> Result<bool> {
        Ok(matches!(
            self.repository.get(SEARCH_AUTO_FOCUS_KEY)?.as_deref(),
            Some("true")
        ))
    }

    pub fn set_search_auto_focus(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            SEARCH_AUTO_FOCUS_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn search_auto_clear(&self) -> Result<bool> {
        Ok(!matches!(
            self.repository.get(SEARCH_AUTO_CLEAR_KEY)?.as_deref(),
            Some("false")
        ))
    }

    pub fn set_search_auto_clear(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            SEARCH_AUTO_CLEAR_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn skip_clear_confirm(&self) -> Result<bool> {
        Ok(matches!(
            self.repository.get(SKIP_CLEAR_CONFIRM_KEY)?.as_deref(),
            Some("true")
        ))
    }

    pub fn set_skip_clear_confirm(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            SKIP_CLEAR_CONFIRM_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn paste_close_window(&self) -> Result<bool> {
        Ok(!matches!(
            self.repository.get(PASTE_CLOSE_WINDOW_KEY)?.as_deref(),
            Some("false")
        ))
    }

    pub fn set_paste_close_window(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            PASTE_CLOSE_WINDOW_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
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
        Ok(!matches!(
            self.repository.get(PASTE_MOVE_TO_TOP_KEY)?.as_deref(),
            Some("false")
        ))
    }

    pub fn set_paste_move_to_top(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            PASTE_MOVE_TO_TOP_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn quick_paste_enabled(&self) -> Result<bool> {
        Ok(!matches!(
            self.repository.get(QUICK_PASTE_ENABLED_KEY)?.as_deref(),
            Some("false")
        ))
    }

    pub fn set_quick_paste_enabled(&self, enabled: bool) -> Result<()> {
        self.repository.set(
            QUICK_PASTE_ENABLED_KEY,
            if enabled { "true" } else { "false" },
        )?;
        Ok(())
    }

    pub fn paste_shortcuts(&self) -> Result<PasteShortcutConfig> {
        Ok(self
            .repository
            .get(PASTE_SHORTCUTS_KEY)?
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default())
    }

    pub fn set_paste_shortcuts(&self, shortcuts: &PasteShortcutConfig) -> Result<()> {
        self.repository
            .set(PASTE_SHORTCUTS_KEY, &serde_json::to_string(shortcuts)?)?;
        Ok(())
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
        Ok(self
            .repository
            .get(HOVER_PREVIEW_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str::<HoverPreviewPreference>(value).ok())
            .filter(|preference| preference.valid())
            .unwrap_or_default())
    }

    pub fn set_hover_preview(&self, preference: HoverPreviewPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "悬停预览设置超出允许范围");
        self.repository
            .set(HOVER_PREVIEW_KEY, &serde_json::to_string(&preference)?)?;
        Ok(())
    }

    pub fn toolbar(&self) -> Result<ToolbarPreference> {
        Ok(self
            .repository
            .get(TOOLBAR_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str::<ToolbarPreference>(value).ok())
            .filter(|preference| preference.valid())
            .unwrap_or_default())
    }

    pub fn set_toolbar(&self, preference: ToolbarPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "工具栏配置无效");
        self.repository
            .set(TOOLBAR_KEY, &serde_json::to_string(&preference)?)?;
        Ok(())
    }

    pub fn display(&self) -> Result<DisplayPreference> {
        Ok(self
            .repository
            .get(DISPLAY_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str::<DisplayPreference>(value).ok())
            .filter(|preference| preference.valid())
            .unwrap_or_default())
    }

    pub fn set_display(&self, preference: DisplayPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "显示设置超出允许范围");
        self.repository
            .set(DISPLAY_KEY, &serde_json::to_string(&preference)?)?;
        Ok(())
    }

    pub fn audio(&self) -> Result<AudioPreference> {
        Ok(self
            .repository
            .get(AUDIO_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default())
    }

    pub fn set_audio(&self, preference: AudioPreference) -> Result<()> {
        self.repository
            .set(AUDIO_KEY, &serde_json::to_string(&preference)?)?;
        Ok(())
    }

    pub fn monitor_types(&self) -> Result<MonitorTypesPreference> {
        Ok(self
            .repository
            .get(MONITOR_TYPES_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str::<MonitorTypesPreference>(value).ok())
            .filter(|preference| preference.valid())
            .unwrap_or_default())
    }

    pub fn set_monitor_types(&self, preference: MonitorTypesPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "至少需要监听一种内容类型");
        self.repository
            .set(MONITOR_TYPES_KEY, &serde_json::to_string(&preference)?)?;
        Ok(())
    }

    pub fn app_filter(&self) -> Result<AppFilterPreference> {
        Ok(self
            .repository
            .get(APP_FILTER_KEY)?
            .as_deref()
            .and_then(|value| serde_json::from_str::<AppFilterPreference>(value).ok())
            .filter(AppFilterPreference::valid)
            .unwrap_or_default())
    }

    pub fn set_app_filter(&self, preference: &AppFilterPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "应用过滤规则无效");
        self.repository
            .set(APP_FILTER_KEY, &serde_json::to_string(preference)?)?;
        Ok(())
    }

    pub fn onboarding_completed(&self) -> Result<bool> {
        Ok(matches!(
            self.repository.get(ONBOARDING_COMPLETED_KEY)?.as_deref(),
            Some("true")
        ))
    }

    pub fn set_onboarding_completed(&self) -> Result<()> {
        self.repository.set(ONBOARDING_COMPLETED_KEY, "true")?;
        Ok(())
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
    fn language_defaults_to_chinese_and_persists_english() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.language()?, LanguagePreference::Chinese);
        preferences.set_language(LanguagePreference::English)?;
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.language()?, LanguagePreference::English);
        SettingsRepository::new(&db).set(LANGUAGE_KEY, "unknown")?;
        assert_eq!(preferences.language()?, LanguagePreference::Chinese);
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

    #[test]
    fn window_behavior_defaults_and_persists_without_restoring_disabled_size() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert!(preferences.persist_window_size()?);
        assert!(!preferences.auto_reset_state()?);
        assert!(!preferences.search_auto_focus()?);
        assert!(preferences.search_auto_clear()?);
        assert!(!preferences.skip_clear_confirm()?);
        assert!(preferences.paste_close_window()?);
        assert_eq!(preferences.paste_key()?, PasteKeyPreference::CtrlV);
        assert!(preferences.paste_move_to_top()?);
        assert!(preferences.quick_paste_enabled()?);
        let size = WindowSizePreference::new(900, 700).unwrap();
        preferences.set_window_size(size)?;
        preferences.set_persist_window_size(false)?;
        preferences.set_auto_reset_state(true)?;
        preferences.set_search_auto_focus(true)?;
        preferences.set_search_auto_clear(false)?;
        preferences.set_skip_clear_confirm(true)?;
        preferences.set_paste_close_window(false)?;
        preferences.set_paste_key(PasteKeyPreference::ShiftInsert)?;
        preferences.set_paste_move_to_top(false)?;
        preferences.set_quick_paste_enabled(false)?;
        assert_eq!(preferences.window_size()?, None);
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert!(!preferences.persist_window_size()?);
        assert!(preferences.auto_reset_state()?);
        assert!(preferences.search_auto_focus()?);
        assert!(!preferences.search_auto_clear()?);
        assert!(preferences.skip_clear_confirm()?);
        assert!(!preferences.paste_close_window()?);
        assert_eq!(preferences.paste_key()?, PasteKeyPreference::ShiftInsert);
        assert!(!preferences.paste_move_to_top()?);
        assert!(!preferences.quick_paste_enabled()?);
        preferences.set_persist_window_size(true)?;
        assert_eq!(preferences.window_size()?, None);
        preferences.set_window_size(size)?;
        assert_eq!(preferences.window_size()?, Some(size));
        Ok(())
    }

    #[test]
    fn paste_shortcut_slots_persist_and_invalid_stored_json_uses_defaults() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        let mut shortcuts = preferences.paste_shortcuts()?;
        assert_eq!(shortcuts.slot(false, 10), Some("Alt+0"));
        assert_eq!(shortcuts.slot(true, 4), Some(""));
        assert_eq!(shortcuts.slot(true, 11), None);
        shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
        shortcuts.set_slot(false, 1, String::new())?;
        assert!(shortcuts.set_slot(true, 0, String::new()).is_err());
        preferences.set_paste_shortcuts(&shortcuts)?;
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.paste_shortcuts()?, shortcuts);
        SettingsRepository::new(&db).set(PASTE_SHORTCUTS_KEY, "invalid")?;
        assert_eq!(
            preferences.paste_shortcuts()?,
            PasteShortcutConfig::default()
        );
        Ok(())
    }

    #[test]
    fn window_position_persists_and_unknown_values_follow_cursor() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(
            preferences.window_position()?,
            WindowPositionPreference::FollowCursor
        );
        preferences.set_window_position(WindowPositionPreference::ScreenCenter)?;
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(
            preferences.window_position()?,
            WindowPositionPreference::ScreenCenter
        );
        preferences.set_window_position(WindowPositionPreference::FixedPosition)?;
        assert_eq!(
            preferences.window_position()?,
            WindowPositionPreference::FixedPosition
        );
        SettingsRepository::new(&db).set(WINDOW_POSITION_KEY, "unknown")?;
        assert_eq!(
            preferences.window_position()?,
            WindowPositionPreference::FollowCursor
        );
        Ok(())
    }

    #[test]
    fn hover_preview_defaults_persist_and_reject_invalid_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(
            preferences.hover_preview()?,
            HoverPreviewPreference::default()
        );
        assert!(preferences.hover_preview()?.allows("image"));
        assert!(!preferences.hover_preview()?.allows("text"));
        let preference = HoverPreviewPreference {
            text: true,
            files: true,
            expanded_image: true,
            delay_ms: 750,
            position: HoverPreviewPosition::Left,
            zoom_step: 20,
            ..HoverPreviewPreference::default()
        };
        preferences.set_hover_preview(preference)?;
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.hover_preview()?, preference);
        assert!(preferences.hover_preview()?.allows("rtf"));
        assert!(
            preferences
                .set_hover_preview(HoverPreviewPreference {
                    delay_ms: 99,
                    ..preference
                })
                .is_err()
        );
        assert_eq!(preferences.hover_preview()?, preference);
        SettingsRepository::new(&db).set(HOVER_PREVIEW_KEY, "invalid")?;
        assert_eq!(
            preferences.hover_preview()?,
            HoverPreviewPreference::default()
        );
        Ok(())
    }

    #[test]
    fn toolbar_order_and_visibility_persist_with_settings_access() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        let toolbar = preferences
            .toolbar()?
            .move_button(ToolbarButton::Settings, -1)
            .unwrap()
            .with_visibility(ToolbarButton::Clear, false)
            .unwrap();
        preferences.set_toolbar(toolbar)?;
        assert!(
            toolbar
                .with_visibility(ToolbarButton::Settings, false)
                .is_none()
        );
        let mut duplicate = toolbar;
        duplicate.items[0].button = ToolbarButton::Settings;
        assert!(preferences.set_toolbar(duplicate).is_err());
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.toolbar()?, toolbar);
        SettingsRepository::new(&db).set(TOOLBAR_KEY, "invalid")?;
        assert_eq!(preferences.toolbar()?, ToolbarPreference::default());
        Ok(())
    }

    #[test]
    fn toolbar_drag_reorders_on_both_sides_of_a_target() {
        let toolbar = ToolbarPreference::default();
        let moved = toolbar
            .move_before_or_after(ToolbarButton::Clear, ToolbarButton::Settings, true)
            .expect("move first to end");
        assert_eq!(moved.items[3].button, ToolbarButton::Clear);
        assert_eq!(moved.items[2].button, ToolbarButton::Settings);

        let moved = moved
            .move_before_or_after(ToolbarButton::Clear, ToolbarButton::Batch, false)
            .expect("move last before batch");
        assert_eq!(moved.items[0].button, ToolbarButton::Clear);
        assert_eq!(moved.items[1].button, ToolbarButton::Batch);
        assert!(moved.valid());
        assert!(
            moved
                .move_before_or_after(ToolbarButton::Clear, ToolbarButton::Batch, false)
                .is_none()
        );
    }

    #[test]
    fn display_preferences_survive_restart_and_recover_invalid_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.display()?, DisplayPreference::default());
        let display = DisplayPreference {
            show_category_filter: false,
            show_drag_area_indicator: false,
            card_density: CardDensity::Compact,
            card_max_lines: 5,
            show_time: false,
            time_format: TimeFormat::Relative,
            show_char_count: false,
            show_byte_size: false,
            show_source_app: false,
            source_app_display: SourceAppDisplay::Icon,
        };
        preferences.set_display(display)?;
        assert!(
            preferences
                .set_display(DisplayPreference {
                    card_max_lines: 11,
                    ..display
                })
                .is_err()
        );
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.display()?, display);
        SettingsRepository::new(&db).set(DISPLAY_KEY, "invalid")?;
        assert_eq!(preferences.display()?, DisplayPreference::default());
        SettingsRepository::new(&db).set(
            DISPLAY_KEY,
            r#"{"show_category_filter":false,"card_density":"compact"}"#,
        )?;
        assert_eq!(
            preferences.display()?,
            DisplayPreference {
                show_category_filter: false,
                card_density: CardDensity::Compact,
                ..DisplayPreference::default()
            }
        );
        Ok(())
    }

    #[test]
    fn audio_preferences_survive_restart_and_recover_invalid_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.audio()?, AudioPreference::default());
        let audio = AudioPreference {
            copy_enabled: true,
            copy_timing: SoundTiming::AfterSuccess,
            paste_enabled: true,
            paste_timing: SoundTiming::Immediate,
        };
        preferences.set_audio(audio)?;
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.audio()?, audio);
        SettingsRepository::new(&db).set(AUDIO_KEY, "invalid")?;
        assert_eq!(preferences.audio()?, AudioPreference::default());
        Ok(())
    }

    #[test]
    fn monitor_types_survive_restart_and_reject_disabling_everything() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(
            preferences.monitor_types()?,
            MonitorTypesPreference::default()
        );
        let types = MonitorTypesPreference {
            text: false,
            url: true,
            html: false,
            rtf: false,
            image: false,
            files: false,
        };
        preferences.set_monitor_types(types)?;
        assert!(
            preferences
                .set_monitor_types(MonitorTypesPreference {
                    url: false,
                    ..types
                })
                .is_err()
        );
        drop(preferences);
        drop(db);

        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.monitor_types()?, types);
        SettingsRepository::new(&db).set(MONITOR_TYPES_KEY, "broken")?;
        assert_eq!(
            preferences.monitor_types()?,
            MonitorTypesPreference::default()
        );
        Ok(())
    }

    #[test]
    fn app_filter_matches_names_processes_paths_and_wildcards() {
        let blacklist = AppFilterPreference::default()
            .with_rule("notepad.exe")
            .unwrap()
            .with_rule("*\\Private\\*")
            .unwrap();
        let blacklist = AppFilterPreference {
            enabled: true,
            ..blacklist
        };
        assert!(blacklist.excludes(Some("Notepad"), Some("C:\\Windows\\notepad.exe")));
        assert!(blacklist.excludes(Some("Other"), Some("C:\\Private\\Editor.exe")));
        assert!(!blacklist.excludes(Some("Browser"), Some("C:\\Apps\\browser.exe")));
        assert!(!blacklist.excludes(None, None));
        let whitelist = AppFilterPreference {
            mode: AppFilterMode::Whitelist,
            ..blacklist
        };
        assert!(!whitelist.excludes(Some("Notepad"), Some("C:\\Windows\\notepad.exe")));
        assert!(whitelist.excludes(Some("Browser"), Some("C:\\Apps\\browser.exe")));
        assert!(!whitelist.excludes(None, None));
        assert!(
            !AppFilterPreference {
                rules: vec![" ".into()],
                ..whitelist
            }
            .valid()
        );
        assert_eq!(
            whitelist.clone().with_rule("NOTEPAD.EXE").unwrap(),
            whitelist
        );
    }

    #[test]
    fn app_filter_persists_and_recovers_invalid_json() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        let filter = AppFilterPreference {
            enabled: true,
            mode: AppFilterMode::Whitelist,
            rules: vec!["Code.exe".into(), "*\\Work\\*".into()],
        };
        preferences.set_app_filter(&filter)?;
        assert!(
            preferences
                .set_app_filter(&AppFilterPreference {
                    rules: vec!["bad\nrule".into()],
                    ..filter.clone()
                })
                .is_err()
        );
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.app_filter()?, filter);
        SettingsRepository::new(&db).set(APP_FILTER_KEY, "broken")?;
        assert_eq!(preferences.app_filter()?, AppFilterPreference::default());
        Ok(())
    }

    #[test]
    fn onboarding_completion_is_saved_in_gpui_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert!(!preferences.onboarding_completed()?);
        preferences.set_onboarding_completed()?;
        drop(preferences);
        drop(db);
        let db = Database::new(path)?;
        assert!(Preferences::new(&db).onboarding_completed()?);
        Ok(())
    }
}
