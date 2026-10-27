//! Windows integration for GPUI windows, shell actions, and native input.

pub(crate) mod acrylic;
pub(crate) mod input;
pub(crate) mod links;
pub(crate) mod paste;
pub(crate) mod position;
pub(crate) mod sound;
pub(crate) mod window;

pub(crate) fn foreground_target() -> Option<(isize, u32)> {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return None;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
    (process_id != 0).then_some((foreground.0 as isize, process_id))
}

/// Distinguish another application's foreground window from our own popups.
pub(crate) fn foreground_window_is_ours() -> bool {
    foreground_target().is_some_and(|target| target.1 == std::process::id())
}
