use ::windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU},
        WindowsAndMessaging::{
            IsIconic, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW, SetForegroundWindow,
            ShowWindow,
        },
    },
};
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn set_window_visible(window: &Window, visible: bool) {
    if let Some(hwnd) = window_hwnd(window) {
        unsafe {
            let command = if !visible {
                SW_HIDE
            } else if IsIconic(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(hwnd, command);
        }
        if visible {
            if unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0 {
                // GPUI activation injects Alt down/up, which releases a held Alt+C
                // chord and prevents pressing C again until Alt is physically reset.
                unsafe {
                    let _ = SetForegroundWindow(hwnd);
                }
            } else {
                window.activate_window();
            }
        }
    }
}

pub(super) fn window_hwnd(window: &Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    }
}

pub fn is_window_shown(window: &Window) -> bool {
    window_hwnd(window).is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) }.as_bool())
}

pub fn is_window_minimized(window: &Window) -> bool {
    window_hwnd(window).is_some_and(|hwnd| unsafe { IsIconic(hwnd) }.as_bool())
}

/// Opaque window identifier passed to clipboard-platform native operations.
pub(crate) fn window_id(window: &Window) -> Option<isize> {
    window_hwnd(window).map(|hwnd| hwnd.0 as isize)
}

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
