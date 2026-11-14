use ::windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ClientToScreen,
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            SendInput, SetActiveWindow, SetFocus, VK_MENU,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GWL_EXSTYLE, GetClientRect, GetWindowLongPtrW, GetWindowRect, HTCLIENT, HWND_NOTOPMOST,
            HWND_TOPMOST, IsIconic, IsWindowVisible, MA_NOACTIVATE, SW_HIDE, SW_RESTORE, SW_SHOW,
            SW_SHOWNA, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_MOUSEACTIVATE,
            WM_NCDESTROY, WM_NCHITTEST, WS_EX_NOACTIVATE,
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

pub(crate) fn log_native_geometry(hwnd: isize, operation: &'static str) {
    let handle = HWND(hwnd as *mut _);
    let mut outer = RECT::default();
    let mut client = RECT::default();
    let result = unsafe {
        GetWindowRect(handle, &mut outer).and_then(|()| GetClientRect(handle, &mut client))
    };
    match result {
        Ok(()) => tracing::debug!(
            operation,
            native_window = hwnd,
            screen_x = outer.left,
            screen_y = outer.top,
            outer_width = outer.right - outer.left,
            outer_height = outer.bottom - outer.top,
            client_width = client.right - client.left,
            client_height = client.bottom - client.top,
            dpi = unsafe { GetDpiForWindow(handle) },
            "Native window geometry"
        ),
        Err(error) => {
            tracing::warn!(operation, native_window = hwnd, %error, "Native geometry unavailable")
        }
    }
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
        let physical = |value: Pixels| (f32::from(value) * self.scale).round() as i32;
        unsafe {
            GetWindowRect(self.hwnd, &mut outer)?;
            GetClientRect(self.hwnd, &mut client)?;
            ClientToScreen(self.hwnd, &mut origin).ok()?;
            SetWindowPos(
                self.hwnd,
                None,
                physical(bounds.origin.x) - (origin.x - outer.left),
                physical(bounds.origin.y) - (origin.y - outer.top),
                physical(bounds.size.width) + (outer.right - outer.left - client.right),
                physical(bounds.size.height) + (outer.bottom - outer.top - client.bottom),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .context("无法初始化窗口尺寸")
    }

    /// Show above other windows without taking focus from the paste target.
    pub fn show(self) {
        unsafe {
            let command = if IsIconic(self.hwnd).as_bool() {
                SW_SHOWNOACTIVATE
            } else {
                SW_SHOWNA
            };
            let _ = ShowWindow(self.hwnd, command);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

/// Take keyboard focus for text input, then drop the topmost band so later
/// windows (settings, dialogs) can stack above the history window again.
pub fn activate_for_input(window: &Window) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    unsafe {
        let _ = SetActiveWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if !SetForegroundWindow(hwnd).as_bool() && GetAsyncKeyState(i32::from(VK_MENU.0)) >= 0 {
            // Preserve the foreground fallback, but never release a held Alt chord.
            let key_down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        ..Default::default()
                    },
                },
            };
            let mut key_up = key_down;
            key_up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
            SendInput(&[key_down, key_up], std::mem::size_of::<INPUT>() as i32);
            let _ = SetForegroundWindow(hwnd);
        }
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
}

/// Keep the native foreground/caret on the paste target during mouse clicks.
/// MA_NOACTIVATE alone can leave the foreground NULL before paste delivery.
/// Explicit activation for text input still works with WS_EX_NOACTIVATE.
pub(crate) fn suppress_click_activation(handle: isize) -> Result<()> {
    let hwnd = HWND(handle as *mut _);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) | WS_EX_NOACTIVATE.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
        anyhow::ensure!(
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) == style,
            "无法设置历史窗口扩展样式"
        );
    }
    anyhow::ensure!(
        unsafe { SetWindowSubclass(hwnd, Some(click_proc), 3, 0) }.as_bool(),
        "无法设置历史窗口消息处理"
    );
    Ok(())
}

unsafe extern "system" fn click_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE if !super::foreground_window_is_ours() => {
                LRESULT(MA_NOACTIVATE as isize)
            }
            // GPUI's hit test needs the App borrow the probing caller already holds.
            // Any opaque answer gives WindowFromPoint the same result.
            WM_NCHITTEST if clipboard_platform::external_drag::window_probe_active() => {
                LRESULT(HTCLIENT as isize)
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(click_proc), id);
                DefSubclassProc(hwnd, msg, w, l)
            }
            _ => DefSubclassProc(hwnd, msg, w, l),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod interaction_tests;
