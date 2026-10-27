//! Windows 快捷键解析与冲突校验（从 hotkey.rs 拆分，保持 500 行限制）。

use anyhow::{Context, Result, anyhow};
use clipboard_core::preferences::{HotkeyPreference, PasteShortcutConfig};
use std::collections::HashSet;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};

use super::hotkey::main_hotkey_binding;

pub(super) struct ParsedPasteShortcut {
    pub(super) canonical: String,
    pub(super) modifiers: HOT_KEY_MODIFIERS,
    pub(super) key: u32,
    pub(super) numpad_key: Option<u32>,
}

pub(super) fn parse_paste_shortcut(value: &str) -> Result<ParsedPasteShortcut> {
    parse_shortcut(value, false)
}

pub fn normalize_main_shortcut(value: &str) -> Result<String> {
    Ok(parse_shortcut(value, true)?.canonical)
}

pub(super) fn parse_shortcut(value: &str, main: bool) -> Result<ParsedPasteShortcut> {
    let parts: Vec<_> = value.split('+').map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        return Err(anyhow!("快捷键需要修饰键和主键"));
    }
    let mut control = false;
    let mut alt = false;
    let mut shift = false;
    let mut win = false;
    for modifier in &parts[..parts.len() - 1] {
        let used = match modifier.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => &mut control,
            "ALT" => &mut alt,
            "SHIFT" => &mut shift,
            "WIN" | "WINDOWS" | "SUPER" if main => &mut win,
            _ => return Err(anyhow!("不支持的快捷键修饰键：{modifier}")),
        };
        if *used {
            return Err(anyhow!("快捷键修饰键重复：{modifier}"));
        }
        *used = true;
    }
    if !control && !alt && !win {
        return Err(anyhow!(if main {
            "快捷键至少需要 Ctrl、Alt 或 Win"
        } else {
            "快速粘贴快捷键至少需要 Ctrl 或 Alt"
        }));
    }
    let key_name = parts[parts.len() - 1].to_ascii_uppercase();
    let (key, numpad_key, key_label) =
        if key_name.len() == 1 && key_name.as_bytes()[0].is_ascii_alphanumeric() {
            let key = u32::from(key_name.as_bytes()[0]);
            let numpad = key_name.as_bytes()[0]
                .is_ascii_digit()
                .then(|| 0x60 + u32::from(key_name.as_bytes()[0] - b'0'));
            (key, numpad, key_name)
        } else if let Some(digit) = key_name
            .strip_prefix("NUMPAD")
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|digit| *digit <= 9)
        {
            (0x60 + u32::from(digit), None, format!("Numpad{digit}"))
        } else if let Some(function) = key_name
            .strip_prefix('F')
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|function| (1..=24).contains(function))
        {
            (0x6f + u32::from(function), None, format!("F{function}"))
        } else {
            let key = match key_name.as_str() {
                "SPACE" => 0x20,
                "TAB" => 0x09,
                "ENTER" => 0x0d,
                "ESC" | "ESCAPE" => 0x1b,
                "BACKSPACE" => 0x08,
                "DELETE" => 0x2e,
                "INSERT" => 0x2d,
                "HOME" => 0x24,
                "END" => 0x23,
                "PAGEUP" => 0x21,
                "PAGEDOWN" => 0x22,
                "LEFT" => 0x25,
                "UP" => 0x26,
                "RIGHT" => 0x27,
                "DOWN" => 0x28,
                _ => return Err(anyhow!("不支持的快捷键主键：{}", parts[parts.len() - 1])),
            };
            (key, None, key_name)
        };
    let mut modifiers = HOT_KEY_MODIFIERS(0);
    let mut canonical = Vec::new();
    if control {
        modifiers |= MOD_CONTROL;
        canonical.push("Ctrl");
    }
    if alt {
        modifiers |= MOD_ALT;
        canonical.push("Alt");
    }
    if shift {
        modifiers |= MOD_SHIFT;
        canonical.push("Shift");
    }
    if win {
        modifiers |= MOD_WIN;
        canonical.push("Win");
    }
    canonical.push(&key_label);
    Ok(ParsedPasteShortcut {
        canonical: canonical.join("+"),
        modifiers,
        key,
        numpad_key,
    })
}

pub fn normalize_paste_shortcut(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(parse_paste_shortcut(value)?.canonical)
}

pub fn validate_paste_shortcuts(
    shortcuts: &PasteShortcutConfig,
    main_hotkey: &HotkeyPreference,
) -> Result<()> {
    let main = main_hotkey_binding(main_hotkey)?.map(|(modifiers, key)| (modifiers.0, key));
    let mut used = HashSet::new();
    for favorite in [false, true] {
        for slot in 1..=10u8 {
            let value = shortcuts.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let parsed =
                parse_paste_shortcut(value).with_context(|| format!("槽位 {slot} 的快捷键无效"))?;
            let primary = (parsed.modifiers.0, parsed.key);
            if Some(primary) == main
                || parsed
                    .numpad_key
                    .is_some_and(|key| Some((parsed.modifiers.0, key)) == main)
            {
                return Err(anyhow!("{} 与窗口唤出快捷键冲突", parsed.canonical));
            }
            if !used.insert(primary) {
                return Err(anyhow!("{} 被多个快速粘贴槽位使用", parsed.canonical));
            }
            if let Some(numpad_key) = parsed.numpad_key
                && !used.insert((parsed.modifiers.0, numpad_key))
            {
                return Err(anyhow!(
                    "{} 的数字小键盘变体与其他槽位冲突",
                    parsed.canonical
                ));
            }
        }
    }
    Ok(())
}

/// 序列粘贴推进键（决策 0004）与主热键及全部槽位互斥；空串表示停用。
pub fn validate_sequence_shortcut(
    shortcuts: &PasteShortcutConfig,
    sequence_shortcut: &str,
    main_hotkey: &HotkeyPreference,
) -> Result<()> {
    if sequence_shortcut.trim().is_empty() {
        return Ok(());
    }
    let parsed = parse_paste_shortcut(sequence_shortcut)?;
    let main = main_hotkey_binding(main_hotkey)?.map(|(modifiers, key)| (modifiers.0, key));
    if Some((parsed.modifiers.0, parsed.key)) == main {
        return Err(anyhow!("{} 与窗口唤出快捷键冲突", parsed.canonical));
    }
    for favorite in [false, true] {
        for slot in 1..=10u8 {
            let value = shortcuts.slot(favorite, slot).unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            let slot_parsed =
                parse_paste_shortcut(value).with_context(|| format!("槽位 {slot} 的快捷键无效"))?;
            if slot_parsed.modifiers == parsed.modifiers
                && (slot_parsed.key == parsed.key
                    || slot_parsed.numpad_key == Some(parsed.key)
                    || parsed.numpad_key == Some(slot_parsed.key))
            {
                return Err(anyhow!("{} 与快速粘贴槽位冲突", parsed.canonical));
            }
        }
    }
    Ok(())
}
