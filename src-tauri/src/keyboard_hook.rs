//! Window state management for hotkey toggle
//! 
//! This module provides window state tracking for toggle functionality.
//! The actual hotkey handling is done via Tauri's global_shortcut plugin.

use parking_lot::RwLock;

// Window state enum (like QuickClipboard)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Hidden,
    Visible,
}

// Track window state using RwLock (like QuickClipboard)
lazy_static::lazy_static! {
    static ref WINDOW_STATE: RwLock<WindowState> = RwLock::new(WindowState::Hidden);
}

/// Get current window state
pub fn get_window_state() -> WindowState {
    *WINDOW_STATE.read()
}

/// Set window state
pub fn set_window_state(state: WindowState) {
    *WINDOW_STATE.write() = state;
}
