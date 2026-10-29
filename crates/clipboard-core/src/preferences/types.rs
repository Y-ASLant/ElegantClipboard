use serde::{Deserialize, Serialize};

use crate::database::ContentType;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerRadiusPreference {
    #[default]
    System,
    Fixed(u8),
}

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

/// 历史去重策略：相同内容再次复制时是移顶既有条目还是另存一条。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupStrategy {
    #[default]
    MoveToTop,
    AlwaysNew,
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
            card_max_lines: 2,
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
            text: true,
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
        match ContentType::parse(content_type) {
            Some(ContentType::Image) => self.image,
            Some(ContentType::Files) => self.files,
            Some(kind) if kind.is_text_like() => self.text,
            _ => false,
        }
    }

    pub(super) fn valid(self) -> bool {
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

    pub(super) fn parse(value: &str) -> Option<Self> {
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

/// 主窗口与设置窗口的跨平台背景材质；平台层选择原生实现与安全回退。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowEffect {
    #[default]
    Opaque,
    Frosted,
    Mica,
}

/// `opacity` 是背板之上的纯色遮罩浓度，0 全透、100 最实；
/// 它不改变原生模糊半径或界面控件的不透明度。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowEffectPreference {
    pub effect: WindowEffect,
    pub opacity: u8,
}

impl Default for WindowEffectPreference {
    fn default() -> Self {
        Self {
            effect: WindowEffect::Opaque,
            opacity: DEFAULT_WINDOW_EFFECT_OPACITY,
        }
    }
}

pub const MIN_WINDOW_EFFECT_OPACITY: u8 = 0;
pub const MAX_WINDOW_EFFECT_OPACITY: u8 = 100;
pub const DEFAULT_WINDOW_EFFECT_OPACITY: u8 = 72;

impl WindowEffectPreference {
    pub fn valid(&self) -> bool {
        (MIN_WINDOW_EFFECT_OPACITY..=MAX_WINDOW_EFFECT_OPACITY).contains(&self.opacity)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LanguagePreference {
    #[default]
    Chinese,
    English,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyPreference {
    Shortcut(String),
    Disabled,
}

impl Default for HotkeyPreference {
    fn default() -> Self {
        Self::Shortcut("Alt+C".into())
    }
}

impl HotkeyPreference {
    pub fn label(&self) -> &str {
        match self {
            Self::Shortcut(shortcut) => shortcut,
            Self::Disabled => "关闭",
        }
    }
}
