use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Parse a single key string to Code
fn parse_key_code(key: &str) -> Option<Code> {
    // Letters A-Z
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
    // Digits 0-9
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
    // Function keys F1-F12
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

    // Single letter
    if key.len() == 1 {
        let c = key.chars().next()?;
        if c.is_ascii_uppercase() {
            return Some(LETTERS[(c as usize) - ('A' as usize)]);
        }
        if c.is_ascii_digit() {
            return Some(DIGITS[(c as usize) - ('0' as usize)]);
        }
    }

    // Function keys F1-F12
    if key.starts_with('F') && key.len() <= 3 {
        if let Ok(n) = key[1..].parse::<usize>() {
            if n >= 1 && n <= 12 {
                return Some(F_KEYS[n - 1]);
            }
        }
    }

    // Special keys
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

/// Parse shortcut string like "CTRL+SHIFT+V" to Shortcut object
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
