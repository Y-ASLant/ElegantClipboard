use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// 将按键字符串解析为 Code
fn parse_key_code(key: &str) -> Option<Code> {
    // 字母 A-Z
    const LETTERS: [Code; 26] = [
        Code::KeyA,
        Code::KeyB,
        Code::KeyC,
        Code::KeyD,
        Code::KeyE,
        Code::KeyF,
        Code::KeyG,
        Code::KeyH,
        Code::KeyI,
        Code::KeyJ,
        Code::KeyK,
        Code::KeyL,
        Code::KeyM,
        Code::KeyN,
        Code::KeyO,
        Code::KeyP,
        Code::KeyQ,
        Code::KeyR,
        Code::KeyS,
        Code::KeyT,
        Code::KeyU,
        Code::KeyV,
        Code::KeyW,
        Code::KeyX,
        Code::KeyY,
        Code::KeyZ,
    ];
    // 数字 0-9
    const DIGITS: [Code; 10] = [
        Code::Digit0,
        Code::Digit1,
        Code::Digit2,
        Code::Digit3,
        Code::Digit4,
        Code::Digit5,
        Code::Digit6,
        Code::Digit7,
        Code::Digit8,
        Code::Digit9,
    ];
    // 功能键 F1-F12
    const F_KEYS: [Code; 12] = [
        Code::F1,
        Code::F2,
        Code::F3,
        Code::F4,
        Code::F5,
        Code::F6,
        Code::F7,
        Code::F8,
        Code::F9,
        Code::F10,
        Code::F11,
        Code::F12,
    ];

    // 单个字母
    if key.len() == 1 {
        let c = key.chars().next()?;
        if c.is_ascii_uppercase() {
            return Some(LETTERS[(c as usize) - ('A' as usize)]);
        }
        if c.is_ascii_digit() {
            return Some(DIGITS[(c as usize) - ('0' as usize)]);
        }
    }

    // 功能键 F1-F12
    if key.starts_with('F')
        && key.len() <= 3
        && let Ok(n) = key[1..].parse::<usize>()
        && (1..=12).contains(&n)
    {
        return Some(F_KEYS[n - 1]);
    }

    // 小键盘数字 Numpad0-Numpad9
    if let Some(rest) = key.strip_prefix("NUMPAD")
        && let Ok(n) = rest.parse::<usize>()
    {
        const NUMPADS: [Code; 10] = [
            Code::Numpad0,
            Code::Numpad1,
            Code::Numpad2,
            Code::Numpad3,
            Code::Numpad4,
            Code::Numpad5,
            Code::Numpad6,
            Code::Numpad7,
            Code::Numpad8,
            Code::Numpad9,
        ];
        if n <= 9 {
            return Some(NUMPADS[n]);
        }
    }

    // 特殊键
    match key {
        "SPACE" => Some(Code::Space),
        "TAB" => Some(Code::Tab),
        "ENTER" | "RETURN" => Some(Code::Enter),
        "BACKSPACE" => Some(Code::Backspace),
        "DELETE" | "DEL" => Some(Code::Delete),
        "ESCAPE" | "ESC" => Some(Code::Escape),
        "HOME" => Some(Code::Home),
        "END" => Some(Code::End),
        "PAGEUP" => Some(Code::PageUp),
        "PAGEDOWN" => Some(Code::PageDown),
        "UP" | "ARROWUP" => Some(Code::ArrowUp),
        "DOWN" | "ARROWDOWN" => Some(Code::ArrowDown),
        "LEFT" | "ARROWLEFT" => Some(Code::ArrowLeft),
        "RIGHT" | "ARROWRIGHT" => Some(Code::ArrowRight),
        "`" | "BACKQUOTE" => Some(Code::Backquote),
        _ => None,
    }
}

/// 将 "CTRL+SHIFT+V" 格式字符串解析为 Shortcut 对象
pub fn parse_shortcut(shortcut_str: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let mut key_code = None;

    for part in parts {
        let upper = part.to_uppercase();
        match upper.as_str() {
            "CTRL" | "CONTROL" => modifiers |= Modifiers::CONTROL,
            "ALT" => modifiers |= Modifiers::ALT,
            "SHIFT" => modifiers |= Modifiers::SHIFT,
            "WIN" | "SUPER" | "META" | "CMD" => modifiers |= Modifiers::SUPER,
            _ => key_code = parse_key_code(&upper),
        }
    }

    key_code.map(|code| {
        if modifiers.is_empty() {
            Shortcut::new(None, code)
        } else {
            Shortcut::new(Some(modifiers), code)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::parse_shortcut;

    #[test]
    fn single_letter() {
        let s = parse_shortcut("V").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::KeyV);
        assert!(s.mods.is_empty());
    }

    #[test]
    fn ctrl_shift_v() {
        let s = parse_shortcut("CTRL+SHIFT+V").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::KeyV);
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::CONTROL)
        );
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::SHIFT)
        );
    }

    #[test]
    fn alt_key() {
        let s = parse_shortcut("ALT+N").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::KeyN);
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::ALT)
        );
    }

    #[test]
    fn win_modifier() {
        let s = parse_shortcut("WIN+V").unwrap();
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::SUPER)
        );
    }

    #[test]
    fn function_key() {
        let s = parse_shortcut("CTRL+F12").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::F12);
    }

    #[test]
    fn special_keys() {
        assert!(parse_shortcut("SPACE").is_some());
        assert!(parse_shortcut("ENTER").is_some());
        assert!(parse_shortcut("RETURN").is_some());
        assert!(parse_shortcut("ESCAPE").is_some());
        assert!(parse_shortcut("ESC").is_some());
        assert!(parse_shortcut("DELETE").is_some());
        assert!(parse_shortcut("DEL").is_some());
        assert!(parse_shortcut("TAB").is_some());
    }

    #[test]
    fn arrow_keys() {
        assert!(parse_shortcut("UP").is_some());
        assert!(parse_shortcut("ARROWUP").is_some());
        assert!(parse_shortcut("DOWN").is_some());
        assert!(parse_shortcut("LEFT").is_some());
        assert!(parse_shortcut("RIGHT").is_some());
    }

    #[test]
    fn numpad() {
        let s = parse_shortcut("NUMPAD5").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::Numpad5);
    }

    #[test]
    fn digits() {
        let s = parse_shortcut("CTRL+1").unwrap();
        assert_eq!(s.key, tauri_plugin_global_shortcut::Code::Digit1);
    }

    #[test]
    fn invalid_key_returns_none() {
        assert!(parse_shortcut("INVALID_KEY").is_none());
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(parse_shortcut("").is_none());
    }

    #[test]
    fn case_insensitive() {
        let s = parse_shortcut("ctrl+shift+v").unwrap();
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::CONTROL)
        );
    }

    #[test]
    fn with_spaces() {
        let s = parse_shortcut("CTRL + SHIFT + V").unwrap();
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::CONTROL)
        );
        assert!(
            s.mods
                .contains(tauri_plugin_global_shortcut::Modifiers::SHIFT)
        );
    }
}
