/// A key pressed while the history window is shown without keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterceptedKey {
    /// A navigation shortcut, named with GPUI keystroke key names.
    Shortcut {
        key: &'static str,
        control: bool,
        shift: bool,
    },
    /// A printable key, which the UI may replay after taking focus for its IME.
    Text(InterceptedText),
}

/// A printable key captured before the history window takes native focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterceptedText {
    pub(crate) keycode: u16,
    #[cfg(windows)]
    pub(crate) scan_code: u16,
    #[cfg(windows)]
    pub(crate) extended: bool,
    /// Printable ASCII translated at key-down, for search without native focus.
    #[cfg(windows)]
    pub search_character: Option<char>,
    /// Whether this is Space with no shortcut modifiers.
    pub unmodified_space: bool,
    /// Whether the key is an auto-repeat of an already pressed key.
    pub is_held: bool,
}

/// Navigation keys forwarded without focus, as GPUI key names.
pub(crate) const NAVIGATION_KEYS: [&str; 11] = [
    "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "enter", "delete", "escape",
];

/// The control shortcut forwarded without focus for `letter` (Ctrl+A, Ctrl+F).
pub(crate) fn control_shortcut(letter: char) -> Option<&'static str> {
    match letter.to_ascii_lowercase() {
        'a' => Some("a"),
        'f' => Some("f"),
        _ => None,
    }
}
