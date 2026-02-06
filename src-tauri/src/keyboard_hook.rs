//! Window state management for hotkey toggle
//!
//! This module provides window state tracking for toggle functionality.
//! The actual hotkey handling is done via Tauri's global_shortcut plugin.

use parking_lot::RwLock;
use std::sync::LazyLock;

// Window state enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Hidden,
    Visible,
}

static WINDOW_STATE: LazyLock<RwLock<WindowState>> =
    LazyLock::new(|| RwLock::new(WindowState::Hidden));

/// Get current window state
pub fn get_window_state() -> WindowState {
    *WINDOW_STATE.read()
}

/// Set window state
pub fn set_window_state(state: WindowState) {
    *WINDOW_STATE.write() = state;
}
