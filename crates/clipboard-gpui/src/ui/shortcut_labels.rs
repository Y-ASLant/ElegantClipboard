use super::*;
use gpui_kit::component::kbd::Kbd;
pub(super) fn paste_registration_summary(
    language: LanguagePreference,
    warnings: &[PasteRegistrationWarning],
) -> Option<String> {
    let first = warnings.first()?;
    Some(match language {
        LanguagePreference::Chinese => format!(
            "{} 个快速粘贴组合键不可用（如 {first}）；可在设置 → 快捷键中更换或关闭快速粘贴",
            warnings.len()
        ),
        LanguagePreference::English => format!(
            "{} quick paste shortcuts are unavailable (e.g. {first}); change or disable them in Settings → Shortcuts",
            warnings.len()
        ),
    })
}

pub(super) fn hotkey_label(language: LanguagePreference, choice: &HotkeyPreference) -> &str {
    if *choice == HotkeyPreference::Disabled {
        tr(language, "关闭", "Disabled")
    } else {
        choice.label()
    }
}

pub(super) fn shortcut_kbd(shortcut: &str) -> Option<Kbd> {
    let mut parts = shortcut.split('+');
    let key = parts.next_back()?.trim();
    if key.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::default();
    for part in parts {
        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            modifiers.control = true;
        } else if part.eq_ignore_ascii_case("alt") {
            modifiers.alt = true;
        } else if part.eq_ignore_ascii_case("shift") {
            modifiers.shift = true;
        } else if part.eq_ignore_ascii_case("win") {
            modifiers.platform = true;
        } else {
            return None;
        }
    }
    Some(Kbd::new(Keystroke {
        key: key.to_ascii_lowercase(),
        modifiers,
        ..Default::default()
    }))
}

pub(super) fn shortcut_from_keystroke(
    keystroke: &Keystroke,
    shift_down: bool,
    main: bool,
) -> Result<String, String> {
    let modifiers = keystroke.modifiers;
    if modifiers.function || (!main && modifiers.platform) {
        return Err("不支持此修饰键".into());
    }
    let key = if shift_down {
        match keystroke.key.as_str() {
            "!" => "1",
            "@" => "2",
            "#" => "3",
            "$" => "4",
            "%" => "5",
            "^" => "6",
            "&" => "7",
            "*" => "8",
            "(" => "9",
            ")" => "0",
            key => key,
        }
    } else {
        keystroke.key.as_str()
    };
    let mut parts = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if shift_down {
        parts.push("Shift");
    }
    if modifiers.platform {
        parts.push("Win");
    }
    parts.push(key);
    if main {
        clipboard_platform::hotkey::normalize_main_shortcut(&parts.join("+"))
    } else {
        normalize_paste_shortcut(&parts.join("+"))
    }
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod shortcut_capture_tests {
    use super::shortcut_from_keystroke;
    use gpui_kit::{Keystroke, Modifiers};

    #[test]
    fn gpui_keystrokes_become_paste_shortcuts() {
        let stroke = Keystroke {
            modifiers: Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
            key: "z".into(),
            ..Default::default()
        };
        assert_eq!(
            shortcut_from_keystroke(&stroke, false, false).unwrap(),
            "Ctrl+Alt+Z"
        );
        let shifted_digit = Keystroke {
            modifiers: Modifiers {
                alt: true,
                ..Default::default()
            },
            key: "!".into(),
            ..Default::default()
        };
        assert_eq!(
            shortcut_from_keystroke(&shifted_digit, true, false).unwrap(),
            "Alt+Shift+1"
        );
        let windows = Keystroke {
            modifiers: Modifiers {
                platform: true,
                ..Default::default()
            },
            key: "f9".into(),
            ..Default::default()
        };
        assert_eq!(
            shortcut_from_keystroke(&windows, false, true).unwrap(),
            "Win+F9"
        );
        assert!(shortcut_from_keystroke(&windows, false, false).is_err());
        let bare = Keystroke {
            key: "z".into(),
            ..Default::default()
        };
        assert!(shortcut_from_keystroke(&bare, false, false).is_err());
    }
}
#[cfg(test)]
mod paste_registration_tests {
    use super::{LanguagePreference, PasteRegistrationWarning, paste_registration_summary};

    #[test]
    fn many_failures_produce_one_bounded_status_with_a_representative_error() {
        let warnings = (0..32)
            .map(|index| PasteRegistrationWarning {
                slot: 1,
                favorite: false,
                message: format!("conflict-{index}"),
            })
            .collect::<Vec<_>>();
        for language in [LanguagePreference::Chinese, LanguagePreference::English] {
            assert!(paste_registration_summary(language, &[]).is_none());
            let summary = paste_registration_summary(language, &warnings).unwrap();
            assert!(summary.contains("32"));
            assert!(summary.contains("conflict-0"));
            assert!(!summary.contains("conflict-31"));
            assert!(summary.chars().count() < 170);
        }
    }
}
