/// A key pressed while the history window is shown without keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterceptedKey {
    /// A navigation shortcut, named with GPUI keystroke key names.
    Shortcut {
        key: &'static str,
        control: bool,
        shift: bool,
    },
    /// Text input: the window must take focus (IME needs it) before the queued
    /// keys are replayed with `replay_pending_text`.
    Text,
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
