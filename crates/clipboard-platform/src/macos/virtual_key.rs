//! macOS virtual key codes (`kVK_*` in HIToolbox `Events.h`) for `CGEventSourceKeyState`.
//! Global-hotkey registration uses another path; this table must match physical ANSI keys.

/// Main key token from a normalized shortcut (e.g. `Alt+5` → `5`, `Ctrl+Shift+F2` → `F2`).
///
/// Returns `(key, alias)`: digit tokens carry the ANSI row code plus the keypad alias
/// because hotkey registration expands `5` into both physical keys (Windows/Linux do the
/// same via their parser's `numpad_key`). Non-digit tokens leave `alias` as `0`.
pub(crate) fn code_for_shortcut_key(token: &str) -> Option<(u16, u16)> {
    if token.len() == 1 {
        let byte = token.as_bytes()[0];
        if byte.is_ascii_digit() {
            return ansi_digit(byte).map(|code| (code, keypad_digit(byte).unwrap_or(0)));
        }
        if byte.is_ascii_alphabetic() {
            return ansi_letter(byte).map(|code| (code, 0));
        }
    }
    if let Some(number) = token.strip_prefix('F').and_then(|v| v.parse::<u8>().ok()) {
        return function_key(number).map(|code| (code, 0));
    }
    // 命名键数量很少，逐个 ASCII 大小写不敏感比较即可，不必分配大写串。
    const NAMED: &[(&str, u16)] = &[
        ("ARROWLEFT", 0x7B),
        ("ARROWRIGHT", 0x7C),
        ("ARROWUP", 0x7E),
        ("ARROWDOWN", 0x7D),
        ("ESCAPE", 0x35),
        ("SPACE", 0x31),
        ("TAB", 0x30),
        ("RETURN", 0x24),
        ("ENTER", 0x24),
        ("BACKSPACE", 0x33),
        ("DELETE", 0x75),
        ("INSERT", 0x72),
        ("HOME", 0x73),
        ("END", 0x77),
        ("PAGEUP", 0x74),
        ("PAGEDOWN", 0x79),
    ];
    NAMED
        .iter()
        .find(|(name, _)| token.eq_ignore_ascii_case(name))
        .map(|(_, code)| (*code, 0))
        .or_else(|| named_numpad(token).map(|code| (code, 0)))
}

fn ansi_digit(byte: u8) -> Option<u16> {
    Some(match byte {
        b'0' => 0x1D,
        b'1' => 0x12,
        b'2' => 0x13,
        b'3' => 0x14,
        b'4' => 0x15,
        b'5' => 0x17,
        b'6' => 0x16,
        b'7' => 0x1A,
        b'8' => 0x1C,
        b'9' => 0x19,
        _ => return None,
    })
}

fn ansi_letter(byte: u8) -> Option<u16> {
    Some(match byte.to_ascii_lowercase() {
        b'a' => 0x00,
        b'b' => 0x0B,
        b'c' => 0x08,
        b'd' => 0x02,
        b'e' => 0x0E,
        b'f' => 0x03,
        b'g' => 0x05,
        b'h' => 0x04,
        b'i' => 0x22,
        b'j' => 0x26,
        b'k' => 0x28,
        b'l' => 0x25,
        b'm' => 0x2E,
        b'n' => 0x2D,
        b'o' => 0x1F,
        b'p' => 0x23,
        b'q' => 0x0C,
        b'r' => 0x0F,
        b's' => 0x01,
        b't' => 0x11,
        b'u' => 0x20,
        b'v' => 0x09,
        b'w' => 0x0D,
        b'x' => 0x07,
        b'y' => 0x10,
        b'z' => 0x06,
        _ => return None,
    })
}

fn function_key(number: u8) -> Option<u16> {
    Some(match number {
        1 => 0x7A,
        2 => 0x78,
        3 => 0x63,
        4 => 0x76,
        5 => 0x60,
        6 => 0x61,
        7 => 0x62,
        8 => 0x64,
        9 => 0x65,
        10 => 0x6D,
        11 => 0x67,
        12 => 0x6F,
        13 => 0x69,
        14 => 0x6B,
        15 => 0x6A,
        16 => 0x6E,
        17 => 0x5C,
        18 => 0x57,
        19 => 0x56,
        20 => 0x50,
        _ => return None,
    })
}

fn named_numpad(token: &str) -> Option<u16> {
    let rest = token.strip_prefix("Numpad")?;
    if rest.len() != 1 {
        return None;
    }
    let byte = rest.as_bytes()[0];
    if byte.is_ascii_digit() {
        return keypad_digit(byte);
    }
    match byte.to_ascii_uppercase() {
        b'+' => Some(0x45),
        b'-' => Some(0x4E),
        b'*' => Some(0x43),
        b'/' => Some(0x4B),
        b'=' => Some(0x51),
        b'.' => Some(0x41),
        _ => None,
    }
}

fn keypad_digit(byte: u8) -> Option<u16> {
    Some(match byte {
        b'0' => 0x52,
        b'1' => 0x53,
        b'2' => 0x54,
        b'3' => 0x55,
        b'4' => 0x56,
        b'5' => 0x57,
        b'6' => 0x58,
        b'7' => 0x59,
        b'8' => 0x5B,
        b'9' => 0x5C,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_digit_row_matches_hitoolbox() {
        for (ch, code) in [
            (b'1', 0x12),
            (b'2', 0x13),
            (b'3', 0x14),
            (b'4', 0x15),
            (b'5', 0x17),
            (b'6', 0x16),
            (b'7', 0x1A),
            (b'8', 0x1C),
            (b'9', 0x19),
            (b'0', 0x1D),
        ] {
            assert_eq!(ansi_digit(ch), Some(code), "digit {}", ch as char);
        }
        assert_ne!(ansi_digit(b'5'), Some(0x11 + 5));
        assert_ne!(ansi_digit(b'6'), Some(0x11 + 6));
    }

    #[test]
    fn function_keys_are_not_linear() {
        assert_eq!(function_key(1), Some(0x7A));
        assert_eq!(function_key(2), Some(0x78));
        assert_eq!(function_key(3), Some(0x63));
        assert_ne!(function_key(1), Some(0x7A + 1));
        assert_ne!(function_key(2), Some(0x7A + 2));
    }

    #[test]
    fn letters_use_ansi_layout_not_alphabet_index() {
        assert_eq!(ansi_letter(b'a'), Some(0x00));
        assert_eq!(ansi_letter(b'b'), Some(0x0B));
        assert_ne!(ansi_letter(b'b'), Some(0x01));
    }

    #[test]
    fn parses_shortcut_tail_token() {
        assert_eq!(code_for_shortcut_key("5"), Some((0x17, 0x57)));
        assert_eq!(code_for_shortcut_key("F2"), Some((0x78, 0)));
        assert_eq!(code_for_shortcut_key("ArrowLeft"), Some((0x7B, 0)));
        assert_eq!(code_for_shortcut_key("Backspace"), Some((0x33, 0)));
        assert_eq!(code_for_shortcut_key("Delete"), Some((0x75, 0)));
    }
}
