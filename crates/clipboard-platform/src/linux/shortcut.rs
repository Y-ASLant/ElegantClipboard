use anyhow::{Context, Result, bail};
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use std::collections::HashSet;

pub(super) struct Shortcut {
    pub canonical: String,
    pub modifiers: u16,
    pub key: u32,
    pub numpad: Option<u32>,
}

pub(super) fn parse(value: &str, main: bool) -> Result<Shortcut> {
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        bail!("A shortcut requires modifiers and a key");
    }
    let mut modifiers = 0;
    for part in &parts[..parts.len() - 1] {
        let bit = match part.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => 4,
            "ALT" => 8,
            "SHIFT" => 1,
            "WIN" | "WINDOWS" | "SUPER" if main => 64,
            _ => bail!("Unsupported shortcut modifier: {part}"),
        };
        if modifiers & bit != 0 {
            bail!("Duplicate shortcut modifier: {part}");
        }
        modifiers |= bit;
    }
    if modifiers & (4 | 8 | 64) == 0 {
        bail!("A shortcut requires Ctrl, Alt, or Super");
    }
    let name = parts[parts.len() - 1].to_ascii_uppercase();
    let (key, numpad, label) = if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
        let key = name.as_bytes()[0];
        (
            u32::from(key.to_ascii_lowercase()),
            key.is_ascii_digit().then(|| 0xffb0 + u32::from(key - b'0')),
            name,
        )
    } else if let Some(number) = name
        .strip_prefix("NUMPAD")
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|n| *n <= 9)
    {
        (0xffb0 + u32::from(number), None, format!("Numpad{number}"))
    } else if let Some(number) = name
        .strip_prefix('F')
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|n| (1..=24).contains(n))
    {
        (0xffbd + u32::from(number), None, format!("F{number}"))
    } else {
        let (key, label) = match name.as_str() {
            "SPACE" => (0x20, "Space"),
            "TAB" => (0xff09, "Tab"),
            "ENTER" => (0xff0d, "Enter"),
            "ESC" | "ESCAPE" => (0xff1b, "Esc"),
            "BACKSPACE" => (0xff08, "Backspace"),
            "DELETE" => (0xffff, "Delete"),
            "INSERT" => (0xff63, "Insert"),
            "HOME" => (0xff50, "Home"),
            "END" => (0xff57, "End"),
            "PAGEUP" => (0xff55, "PageUp"),
            "PAGEDOWN" => (0xff56, "PageDown"),
            "LEFT" => (0xff51, "Left"),
            "UP" => (0xff52, "Up"),
            "RIGHT" => (0xff53, "Right"),
            "DOWN" => (0xff54, "Down"),
            _ => bail!("Unsupported shortcut key: {name}"),
        };
        (key, None, label.to_owned())
    };
    let mut canonical = Vec::new();
    for (mask, label) in [(4, "Ctrl"), (8, "Alt"), (1, "Shift"), (64, "Super")] {
        if modifiers & mask != 0 {
            canonical.push(label);
        }
    }
    canonical.push(&label);
    Ok(Shortcut {
        canonical: canonical.join("+"),
        modifiers,
        key,
        numpad,
    })
}

pub fn normalize_main_shortcut(value: &str) -> Result<String> {
    Ok(parse(value, true)?.canonical)
}

pub fn normalize_paste_shortcut(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(parse(value, false)?.canonical)
}

pub fn validate_paste_shortcuts(
    config: &PasteShortcutConfig,
    main: &HotkeyPreference,
) -> Result<()> {
    let main = match main {
        HotkeyPreference::Shortcut(value) => Some(parse(value, true)?),
        HotkeyPreference::Disabled => None,
    };
    let mut used = HashSet::new();
    for favorite in [false, true] {
        for slot in 1..=10 {
            let value = config.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let parsed =
                parse(value, false).with_context(|| format!("Invalid shortcut for slot {slot}"))?;
            for key in std::iter::once(parsed.key).chain(parsed.numpad) {
                if main
                    .as_ref()
                    .is_some_and(|main| main.modifiers == parsed.modifiers && main.key == key)
                {
                    bail!("{} conflicts with the main shortcut", parsed.canonical);
                }
                if !used.insert((parsed.modifiers, key)) {
                    bail!("{} is assigned to multiple slots", parsed.canonical);
                }
            }
        }
    }
    Ok(())
}

/// 序列粘贴推进键（决策 0004）与主热键及全部槽位互斥；空串表示停用。
pub fn validate_sequence_shortcut(
    config: &PasteShortcutConfig,
    sequence_shortcut: &str,
    main: &HotkeyPreference,
) -> Result<()> {
    if sequence_shortcut.trim().is_empty() {
        return Ok(());
    }
    let parsed = parse(sequence_shortcut, false)?;
    let main = match main {
        HotkeyPreference::Shortcut(value) => Some(parse(value, true)?),
        HotkeyPreference::Disabled => None,
    };
    let sequence_keys: Vec<u32> = std::iter::once(parsed.key).chain(parsed.numpad).collect();
    if main
        .as_ref()
        .is_some_and(|main| main.modifiers == parsed.modifiers && sequence_keys.contains(&main.key))
    {
        bail!("{} conflicts with the main shortcut", parsed.canonical);
    }
    for favorite in [false, true] {
        for slot in 1..=10 {
            let value = config.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let slot_parsed =
                parse(value, false).with_context(|| format!("Invalid shortcut for slot {slot}"))?;
            for key in std::iter::once(slot_parsed.key).chain(slot_parsed.numpad) {
                if slot_parsed.modifiers == parsed.modifiers && sequence_keys.contains(&key) {
                    bail!("{} conflicts with a quick-paste slot", parsed.canonical);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_modifiers_and_unmodified_shortcuts() {
        for shortcut in [
            "Shift+A",
            "Ctrl+Control+A",
            "Super+Win+F1",
            "Alt+",
            "Ctrl+F25",
        ] {
            assert!(normalize_main_shortcut(shortcut).is_err());
        }
        assert_eq!(parse("windows+control+f9", true).unwrap().modifiers, 68);
        assert_eq!(parse("Ctrl+A", true).unwrap().key, u32::from(b'a'));
    }

    #[test]
    fn detects_digit_alias_and_main_shortcut_conflicts() {
        let mut shortcuts = PasteShortcutConfig {
            recent: std::array::from_fn(|_| String::new()),
            favorites: std::array::from_fn(|_| String::new()),
        };
        shortcuts.recent[0] = "Alt+1".into();
        shortcuts.favorites[0] = "Alt+Numpad1".into();
        assert!(validate_paste_shortcuts(&shortcuts, &HotkeyPreference::Disabled).is_err());
        shortcuts.favorites[0].clear();
        assert!(
            validate_paste_shortcuts(
                &shortcuts,
                &HotkeyPreference::Shortcut("Alt+Numpad1".into())
            )
            .is_err()
        );
        assert!(
            validate_paste_shortcuts(&shortcuts, &HotkeyPreference::Shortcut("Super+V".into()))
                .is_ok()
        );
    }
}
