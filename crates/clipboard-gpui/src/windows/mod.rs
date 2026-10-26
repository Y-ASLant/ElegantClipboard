//! Windows integration for GPUI windows, shell actions, and native input.

pub(crate) mod acrylic;
pub(crate) mod input;
pub(crate) mod links;
pub(crate) mod paste;
pub(crate) mod position;
pub(crate) mod sound;
pub(crate) mod window;

/// Distinguish another application's foreground window from our own popups.
pub(crate) fn foreground_window_is_ours() -> bool {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return false;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
    process_id == std::process::id()
}
