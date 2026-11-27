use ::windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ClientToScreen, ScreenToClient},
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            SendInput, SetActiveWindow, SetFocus, VK_MENU,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            BringWindowToTop, GWL_EXSTYLE, GetClientRect, GetCursorPos, GetForegroundWindow,
            GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, HTCLIENT, HWND_NOTOPMOST,
            HWND_TOPMOST, IsIconic, IsWindowVisible, MA_ACTIVATE, MA_NOACTIVATE, SW_HIDE,
            SW_RESTORE, SW_SHOW, SW_SHOWNA, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
            ShowWindow, WA_INACTIVE, WM_ACTIVATE, WM_MOUSEACTIVATE, WM_NCDESTROY, WM_NCHITTEST,
            WS_EX_NOACTIVATE, WS_EX_TOPMOST,
        },
    },
};
use anyhow::{Context, Result};
use gpui_kit::{Bounds, Pixels, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy)]
struct ClickActivateRegion {
    bounds: Bounds<Pixels>,
    fallback_scale: f32,
}

#[derive(Clone, Copy, Default)]
struct ClickSuppression {
    enabled: bool,
    activate_region: Option<ClickActivateRegion>,
}

fn click_suppressions() -> &'static Mutex<HashMap<isize, ClickSuppression>> {
    static STATES: OnceLock<Mutex<HashMap<isize, ClickSuppression>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_click_suppression_enabled(handle: isize) -> bool {
    let states = click_suppressions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    states.get(&handle).is_some_and(|state| state.enabled)
}

fn click_activate_region(handle: isize) -> Option<ClickActivateRegion> {
    let states = click_suppressions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    states
        .get(&handle)
        .filter(|state| state.enabled)
        .and_then(|state| state.activate_region)
}

fn set_click_activate_region(handle: isize, bounds: Option<Bounds<Pixels>>, fallback_scale: f32) {
    let mut states = click_suppressions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(state) = states.get_mut(&handle).filter(|state| state.enabled) {
        state.activate_region = bounds.map(|bounds| ClickActivateRegion {
            bounds,
            fallback_scale,
        });
    }
}

fn click_activate_rect_with_scale(handle: isize, live_scale: Option<f32>) -> Option<RECT> {
    let region = click_activate_region(handle)?;
    let scale = live_scale
        .filter(|scale| scale.is_finite() && *scale > 0.)
        .unwrap_or(region.fallback_scale);
    bounds_to_physical_rect(region.bounds, scale)
}

fn live_window_scale(hwnd: HWND) -> Option<f32> {
    // SAFETY: hwnd is a live top-level window; GetDpiForWindow returns 0 when unavailable.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    (dpi != 0).then(|| dpi as f32 / 96.0)
}

fn bounds_to_physical_rect(bounds: Bounds<Pixels>, scale: f32) -> Option<RECT> {
    if !scale.is_finite() || scale <= 0. {
        return None;
    }
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return None;
    }
    let x = f32::from(bounds.origin.x);
    let y = f32::from(bounds.origin.y);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(RECT {
        left: (x * scale).floor() as i32,
        top: (y * scale).floor() as i32,
        right: ((x + width) * scale).ceil() as i32,
        bottom: ((y + height) * scale).ceil() as i32,
    })
}

fn point_in_rect(point: POINT, rect: RECT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

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
        if is_click_suppression_enabled(self.hwnd.0 as isize) {
            set_noactivate_style(self.hwnd, true);
        }
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

/// Register the logical client region where a mouse click should activate the
/// non-activating history window (currently the search input).
pub fn set_click_activate_bounds(window: &Window, bounds: Option<Bounds<Pixels>>) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    set_click_activate_region(hwnd.0 as isize, bounds, window.scale_factor());
}

/// Take keyboard focus for text input. Only the non-activating history panel
/// relinquishes its temporary topmost level; an editor's pin is user-owned.
pub fn activate_for_input(window: &Window) {
    let Some(hwnd) = window_hwnd(window) else {
        return;
    };
    activate_hwnd_for_input(hwnd);
}

fn activate_hwnd_for_input(hwnd: HWND) {
    // Strip WS_EX_NOACTIVATE first so Windows accepts foreground and IME activation.
    set_noactivate_style(hwnd, false);
    let mut foreground = false;
    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        foreground = unsafe {
            GetForegroundWindow() == hwnd
                || (SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd)
        };
        if !foreground {
            unsafe {
                let fg_hwnd = GetForegroundWindow();
                let fg_tid = if fg_hwnd.0.is_null() {
                    0
                } else {
                    GetWindowThreadProcessId(fg_hwnd, None)
                };
                let current_tid = GetCurrentThreadId();
                let attached = fg_tid != 0
                    && fg_tid != current_tid
                    && AttachThreadInput(current_tid, fg_tid, true).as_bool();
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = SetActiveWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
                if attached {
                    let _ = AttachThreadInput(current_tid, fg_tid, false);
                }
            }
            foreground = unsafe { GetForegroundWindow() == hwnd };
        }
        if !foreground && unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } >= 0 {
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
            unsafe {
                SendInput(&[key_down, key_up], std::mem::size_of::<INPUT>() as i32);
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
            }
            foreground = unsafe { GetForegroundWindow() == hwnd };
        }
    }
    if foreground {
        unsafe {
            let _ = SetActiveWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
        }
        if is_click_suppression_enabled(hwnd.0 as isize) {
            drop_topmost_if_set(hwnd);
        }
    } else if is_click_suppression_enabled(hwnd.0 as isize) {
        // Never leave a click-suppressed panel without WS_EX_NOACTIVATE if activation
        // was denied (e.g. UIPI against an elevated foreground window): WM_ACTIVATE
        // will not fire to restore it.
        set_noactivate_style(hwnd, true);
    }
}

/// Keep or drop the topmost band without taking focus.
pub fn set_above(window: &Window, above: bool) {
    if let Some(hwnd) = window_hwnd(window) {
        place(
            hwnd,
            Some(if above { HWND_TOPMOST } else { HWND_NOTOPMOST }),
        );
    }
}

#[derive(Clone, Copy)]
pub struct MainWindowLevel(());

pub fn lower_for_editor(window: &Window) -> Option<MainWindowLevel> {
    drop_topmost_if_set(window_hwnd(window)?).then_some(MainWindowLevel(()))
}

pub fn restore_after_editor(window: &Window, _: MainWindowLevel) {
    set_above(window, true);
}

/// Place the window at the top of the non-topmost band without activating it.
pub fn raise_front(window: &Window) {
    if let Some(hwnd) = window_hwnd(window) {
        // NULL insert-after is HWND_TOP.
        place(hwnd, None);
    }
}

fn place(hwnd: HWND, after: Option<HWND>) -> bool {
    // SAFETY: hwnd is a live GPUI window. Flags only change z-order.
    unsafe {
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
        .is_ok()
    }
}

fn set_noactivate_style(hwnd: HWND, enabled: bool) {
    // SAFETY: hwnd is a live top-level window; toggles only WS_EX_NOACTIVATE.
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let flag = WS_EX_NOACTIVATE.0 as isize;
        let target = if enabled {
            current | flag
        } else {
            current & !flag
        };
        if target != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, target);
        }
    }
}

fn drop_topmost_if_set(hwnd: HWND) -> bool {
    // SAFETY: hwnd is a live window; z-order only changes when WS_EX_TOPMOST is set.
    unsafe {
        (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize) != 0
            && place(hwnd, Some(HWND_NOTOPMOST))
    }
}

fn cursor_client_point(hwnd: HWND) -> Option<POINT> {
    let mut point = POINT::default();
    // SAFETY: hwnd is live and ScreenToClient converts screen coordinates to client coordinates.
    let converted =
        unsafe { GetCursorPos(&mut point).is_ok() && ScreenToClient(hwnd, &mut point).as_bool() };
    converted.then_some(point)
}

fn handle_mouse_activate(
    hwnd: HWND,
    client_point: Option<POINT>,
    live_scale: Option<f32>,
    is_self_foreground: bool,
    ours_is_foreground: bool,
) -> Option<LRESULT> {
    if !is_self_foreground
        && let Some(point) = client_point
        && click_activate_rect_with_scale(hwnd.0 as isize, live_scale)
            .is_some_and(|rect| point_in_rect(point, rect))
    {
        set_noactivate_style(hwnd, false);
        return Some(LRESULT(MA_ACTIVATE as isize));
    }
    if !ours_is_foreground {
        set_noactivate_style(hwnd, true);
        return Some(LRESULT(MA_NOACTIVATE as isize));
    }
    None
}

/// Keep the native foreground/caret on the paste target during mouse clicks.
/// MA_NOACTIVATE alone can leave the foreground NULL before paste delivery.
/// Clicks inside the registered search box region temporarily remove
/// WS_EX_NOACTIVATE and return MA_ACTIVATE; deactivation and re-show restore it.
pub(crate) fn suppress_click_activation(handle: isize) -> Result<()> {
    let hwnd = HWND(handle as *mut _);
    set_noactivate_style(hwnd, true);
    let flag = WS_EX_NOACTIVATE.0 as isize;
    anyhow::ensure!(
        unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } & flag != 0,
        "无法设置历史窗口扩展样式"
    );
    anyhow::ensure!(
        unsafe { SetWindowSubclass(hwnd, Some(click_proc), 3, 0) }.as_bool(),
        "无法设置历史窗口消息处理"
    );
    let mut states = click_suppressions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    states.entry(handle).or_default().enabled = true;
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
            WM_MOUSEACTIVATE => {
                let is_self_foreground = GetForegroundWindow() == hwnd;
                let ours_is_foreground = is_self_foreground || super::foreground_window_is_ours();
                let client_point = (!is_self_foreground)
                    .then(|| cursor_client_point(hwnd))
                    .flatten();
                let live_scale = client_point.and_then(|_| live_window_scale(hwnd));
                if let Some(result) = handle_mouse_activate(
                    hwnd,
                    client_point,
                    live_scale,
                    is_self_foreground,
                    ours_is_foreground,
                ) {
                    result
                } else {
                    DefSubclassProc(hwnd, msg, w, l)
                }
            }
            WM_ACTIVATE => {
                let result = DefSubclassProc(hwnd, msg, w, l);
                if (w.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    set_noactivate_style(hwnd, true);
                } else {
                    drop_topmost_if_set(hwnd);
                }
                result
            }
            // GPUI's hit test needs the App borrow the probing caller already holds.
            // Any opaque answer gives WindowFromPoint the same result.
            WM_NCHITTEST if clipboard_platform::external_drag::window_probe_active() => {
                LRESULT(HTCLIENT as isize)
            }
            WM_NCDESTROY => {
                click_suppressions()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&(hwnd.0 as isize));
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
