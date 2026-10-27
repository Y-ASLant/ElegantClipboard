use super::{cocoa, desktop};
use anyhow::{Context, Result, bail};
use async_channel::Sender;
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use std::{
    collections::{HashMap, HashSet},
    sync::{LazyLock, Mutex, Once},
};

pub use crate::paste_types::{PasteHotkeyEvent, PasteRegistrationWarning, SequenceAdvanceEvent};

enum Destination {
    Main(Sender<(isize, u32)>),
    Paste(Sender<PasteHotkeyEvent>, u8, bool),
    SequenceAdvance(Sender<SequenceAdvanceEvent>),
}

struct Route {
    destination: Destination,
    pressed: bool,
}

fn routes() -> &'static Mutex<HashMap<u32, Route>> {
    static ROUTES: LazyLock<Mutex<HashMap<u32, Route>>> = LazyLock::new(Mutex::default);
    &ROUTES
}

fn install_handler() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        GlobalHotKeyEvent::set_event_handler(Some(|event: GlobalHotKeyEvent| {
            let Ok(mut routes) = routes().lock() else {
                return;
            };
            let Some(route) = routes.get_mut(&event.id()) else {
                return;
            };
            let pressed = event.state() == HotKeyState::Pressed;
            let dispatch = pressed && !route.pressed;
            route.pressed = pressed;
            if !dispatch {
                return;
            }
            // Carbon 快捷键不依赖辅助功能权限；无 AX 目标时仍唤起面板，
            // 粘贴消费者会拒绝窗口令牌为零的目标。
            let target = desktop::foreground_target().unwrap_or((0, 0));
            match &route.destination {
                Destination::Main(events) => {
                    let _ = events.try_send(target);
                }
                Destination::Paste(events, slot, favorite) => {
                    let _ = events.try_send(PasteHotkeyEvent {
                        slot: *slot,
                        favorite: *favorite,
                        target,
                    });
                }
                Destination::SequenceAdvance(events) => {
                    let _ = events.try_send(SequenceAdvanceEvent { target });
                }
            }
        }))
    });
}

struct Registration {
    manager: GlobalHotKeyManager,
    keys: Vec<HotKey>,
}

impl Registration {
    fn new() -> Result<Self> {
        cocoa::main_thread()?;
        install_handler();
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            keys: Vec::new(),
        })
    }

    fn register(&mut self, key: HotKey, destination: Destination) -> Result<()> {
        let mut routes = routes()
            .lock()
            .map_err(|_| anyhow::anyhow!("快捷键路由锁损坏"))?;
        if routes.contains_key(&key.id()) {
            bail!("快捷键已被应用内其他操作使用")
        }
        self.manager
            .register(key)
            .context("macOS 全局快捷键注册失败，可能已被系统或其他应用占用")?;
        routes.insert(
            key.id(),
            Route {
                destination,
                pressed: false,
            },
        );
        self.keys.push(key);
        Ok(())
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut routes) = routes().lock() {
            for key in &self.keys {
                routes.remove(&key.id());
            }
        }
        for key in self.keys.drain(..) {
            let _ = self.manager.unregister(key);
        }
    }
}

pub struct Hotkey {
    _registration: Registration,
}

impl Hotkey {
    pub fn start(choice: &HotkeyPreference, events: Sender<(isize, u32)>) -> Result<Self> {
        let HotkeyPreference::Shortcut(value) = choice else {
            bail!("快捷键已关闭")
        };
        let mut registration = Registration::new()?;
        registration.register(parse(value)?.0, Destination::Main(events))?;
        Ok(Self {
            _registration: registration,
        })
    }
}

fn parse(value: &str) -> Result<(HotKey, String)> {
    let mut parts: Vec<_> = value.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        bail!("快捷键需要修饰键和主键");
    }
    let key = parts.pop().expect("nonempty shortcut");
    let mut modifiers = [false; 4];
    for part in parts {
        let index = match part.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => 0,
            "ALT" | "OPTION" => 1,
            "SHIFT" => 2,
            "WIN" | "WINDOWS" | "SUPER" | "CMD" | "COMMAND" | "META" => 3,
            _ => bail!("不支持的快捷键修饰键：{part}"),
        };
        if modifiers[index] {
            bail!("快捷键修饰键重复：{part}")
        }
        modifiers[index] = true;
    }
    if !modifiers[0] && !modifiers[1] && !modifiers[3] {
        bail!("快捷键至少需要 Ctrl、Alt 或 Command");
    }
    let mut canonical = Vec::new();
    for (used, label) in modifiers.into_iter().zip(["Ctrl", "Alt", "Shift", "Cmd"]) {
        if used {
            canonical.push(label);
        }
    }
    let key = key.to_ascii_uppercase();
    let key = match key.as_str() {
        "LEFT" => "ArrowLeft",
        "RIGHT" => "ArrowRight",
        "UP" => "ArrowUp",
        "DOWN" => "ArrowDown",
        "ESC" => "Escape",
        _ => &key,
    };
    canonical.push(key);
    let canonical = canonical.join("+");
    Ok((canonical.parse().context("macOS 快捷键无效")?, canonical))
}

fn variants(value: &str) -> Result<Vec<HotKey>> {
    let (key, _) = parse(value)?;
    let mut result = vec![key];
    let last = value.rsplit('+').next().unwrap_or_default().trim();
    if last.len() == 1 && last.as_bytes()[0].is_ascii_digit() {
        let prefix = value.rsplit_once('+').expect("validated shortcut").0;
        result.push(parse(&format!("{prefix}+Numpad{last}"))?.0);
    }
    Ok(result)
}

pub fn normalize_main_shortcut(value: &str) -> Result<String> {
    Ok(parse(value)?.1)
}

pub fn normalize_paste_shortcut(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        Ok(String::new())
    } else {
        normalize_main_shortcut(value)
    }
}

pub fn validate_paste_shortcuts(
    shortcuts: &PasteShortcutConfig,
    main: &HotkeyPreference,
) -> Result<()> {
    let main = match main {
        HotkeyPreference::Shortcut(value) => Some(parse(value)?.0.id()),
        _ => None,
    };
    let mut used = HashSet::new();
    for favorite in [false, true] {
        for slot in 1..=10 {
            let value = shortcuts.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            for key in variants(value)? {
                if Some(key.id()) == main {
                    bail!("{value} 与窗口唤出快捷键冲突")
                }
                if !used.insert(key.id()) {
                    bail!("{value} 被多个快速粘贴槽位使用")
                }
            }
        }
    }
    Ok(())
}

/// 序列粘贴推进键（决策 0004）与主热键及全部槽位互斥；空串表示停用。
pub fn validate_sequence_shortcut(
    shortcuts: &PasteShortcutConfig,
    sequence_shortcut: &str,
    main: &HotkeyPreference,
) -> Result<()> {
    if sequence_shortcut.trim().is_empty() {
        return Ok(());
    }
    let key = parse(sequence_shortcut)?.0.id();
    let main = match main {
        HotkeyPreference::Shortcut(value) => Some(parse(value)?.0.id()),
        _ => None,
    };
    if Some(key) == main {
        bail!("{sequence_shortcut} 与窗口唤出快捷键冲突")
    }
    for favorite in [false, true] {
        for slot in 1..=10 {
            let value = shortcuts.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            if variants(value)?.iter().any(|slot_key| slot_key.id() == key) {
                bail!("{sequence_shortcut} 与快速粘贴槽位冲突")
            }
        }
    }
    Ok(())
}

pub struct PasteHotkeys {
    _registration: Registration,
}

impl PasteHotkeys {
    /// `sequence_shortcut` 是序列粘贴推进键（决策 0004）；空串表示停用。
    /// 推进键常驻注册（未激活序列时事件由 UI 忽略）；`slots_enabled` 控制槽位是否注册。
    pub fn start(
        events: Sender<PasteHotkeyEvent>,
        sequence_events: Sender<SequenceAdvanceEvent>,
        shortcuts: &PasteShortcutConfig,
        sequence_shortcut: &str,
        slots_enabled: bool,
        main: &HotkeyPreference,
    ) -> Result<(Self, Vec<PasteRegistrationWarning>)> {
        validate_paste_shortcuts(shortcuts, main)?;
        validate_sequence_shortcut(shortcuts, sequence_shortcut, main)?;
        let mut registration = Registration::new()?;
        let mut warnings = Vec::new();
        if !sequence_shortcut.trim().is_empty()
            && let Err(error) = registration.register(
                parse(sequence_shortcut)?.0,
                Destination::SequenceAdvance(sequence_events),
            )
        {
            warnings.push(PasteRegistrationWarning {
                slot: 0,
                favorite: false,
                message: format!("{sequence_shortcut}: {error:#}"),
            });
        }
        if slots_enabled {
            for favorite in [false, true] {
                for slot in 1..=10 {
                    let value = shortcuts.slot(favorite, slot).unwrap_or_default();
                    if value.is_empty() {
                        continue;
                    }
                    for key in variants(value)? {
                        if let Err(error) = registration
                            .register(key, Destination::Paste(events.clone(), slot, favorite))
                        {
                            warnings.push(PasteRegistrationWarning {
                                slot,
                                favorite,
                                message: format!("{value}: {error:#}"),
                            });
                        }
                    }
                }
            }
        }
        if registration.keys.is_empty() && !warnings.is_empty() {
            bail!("全部快速粘贴快捷键注册失败：{}", warnings[0]);
        }
        Ok((
            Self {
                _registration: registration,
            },
            warnings,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_aliases_share_native_identity() {
        let command = parse("Command+V").unwrap().0.id();
        for alias in ["Cmd+V", "Win+V", "Windows+V", "Super+V", "Meta+V"] {
            assert_eq!(parse(alias).unwrap().0.id(), command);
        }
        assert_ne!(parse("Ctrl+V").unwrap().0.id(), command);
    }

    #[test]
    fn repeated_modifiers_and_shift_only_shortcuts_are_rejected() {
        for invalid in ["Cmd+Win+V", "Ctrl+Control+V", "Shift+V", "Cmd++V", "V"] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn digit_variants_cover_keypad_without_expanding_named_keypad_keys() {
        let digits = variants("Ctrl+Alt+1").unwrap();
        assert_eq!(digits.len(), 2);
        assert_ne!(digits[0].id(), digits[1].id());
        let keypad = variants("Ctrl+Alt+Numpad1").unwrap();
        assert_eq!(keypad.len(), 1);
        assert_eq!(keypad[0].id(), digits[1].id());
    }
}
