use ::windows::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::ClientToScreen,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            SendInput, SetActiveWindow, SetFocus, VK_MENU,
        },
        WindowsAndMessaging::{
            GetClientRect, GetWindowRect, IsIconic, IsWindowVisible, SW_HIDE, SW_RESTORE, SW_SHOW,
            SWP_NOACTIVATE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow,
        },
    },
};
use anyhow::{Context, Result};
use gpui_kit::{Bounds, Pixels, Window};
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

/// Main-window presentation never calls GPUI activation: hidden-created Windows
/// windows retain an initial placement that activation would replay after moving.
pub struct WindowShowRequest {
    hwnd: HWND,
    initial_bounds: Option<Bounds<Pixels>>,
    scale: f32,
}

pub fn prepare_window_show(
    window: &Window,
    initial_bounds: Option<Bounds<Pixels>>,
) -> Result<WindowShowRequest> {
    Ok(WindowShowRequest {
        hwnd: window_hwnd(window).context("无法取得窗口句柄")?,
        initial_bounds,
        scale: window.scale_factor(),
    })
}

impl WindowShowRequest {
    /// Native calls can synchronously reenter GPUI; execute outside all its borrows.
    pub fn initialize(&self) -> Result<()> {
        let Some(bounds) = self.initial_bounds else {
            return Ok(());
        };
        let mut outer = RECT::default();
        let mut client = RECT::default();
        let mut origin = POINT::default();
        unsafe {
            GetWindowRect(self.hwnd, &mut outer)?;
            GetClientRect(self.hwnd, &mut client)?;
            ClientToScreen(self.hwnd, &mut origin).ok()?;
            SetWindowPos(
                self.hwnd,
                None,
                (f32::from(bounds.origin.x) * self.scale).round() as i32 - (origin.x - outer.left),
                (f32::from(bounds.origin.y) * self.scale).round() as i32 - (origin.y - outer.top),
                (f32::from(bounds.size.width) * self.scale).round() as i32
                    + (outer.right - outer.left - client.right),
                (f32::from(bounds.size.height) * self.scale).round() as i32
                    + (outer.bottom - outer.top - client.bottom),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .context("无法初始化窗口尺寸")
    }

    pub fn show(self) {
        unsafe {
            let command = if IsIconic(self.hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(self.hwnd, command);
            let _ = SetActiveWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
            if !SetForegroundWindow(self.hwnd).as_bool()
                && GetAsyncKeyState(i32::from(VK_MENU.0)) >= 0
            {
                // Preserve the foreground fallback, but never release a held Alt chord.
                let mut input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_MENU,
                            ..Default::default()
                        },
                    },
                };
                let down = input;
                input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                SendInput(&[down, input], std::mem::size_of::<INPUT>() as i32);
                let _ = SetForegroundWindow(self.hwnd);
            }
        }
    }
}

#[cfg(test)]
mod tests;
