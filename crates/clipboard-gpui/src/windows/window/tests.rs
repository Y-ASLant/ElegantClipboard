use super::WindowShowRequest;
use gpui_kit::{Bounds, point, px, size};
use windows::{
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetClientRect, GetWindowRect, IsWindowVisible,
            WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
        },
    },
    core::w,
};

struct HiddenWindow(HWND);

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) }.expect("destroy isolated native window");
    }
}

#[test]
fn initial_client_bounds_are_applied_without_exposing_or_replaying_the_window() -> anyhow::Result<()>
{
    // No application window, foreground changes, user data or system clipboard access.
    // The client stays small: Windows clamps a window to the work area, so a large
    // logical size at high scale is not evidence that placement ignored the request.
    let window = HiddenWindow(unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Isolated hidden placement test"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            200,
            200,
            None,
            None,
            None,
            None,
        )?
    });
    let scale = 1.25;
    let request = WindowShowRequest {
        hwnd: window.0,
        initial_bounds: Some(Bounds::new(
            point(px(40.), px(24.)),
            size(px(160.), px(120.)),
        )),
        scale,
    };
    request.initialize()?;
    let mut client = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        GetClientRect(window.0, &mut client)?;
        ClientToScreen(window.0, &mut origin).ok()?;
        assert!(!IsWindowVisible(window.0).as_bool());
    }
    let physical = |value: f32| (value * scale).round() as i32;
    assert_eq!((origin.x, origin.y), (physical(40.), physical(24.)));
    assert_eq!(
        (client.right, client.bottom),
        (physical(160.), physical(120.))
    );

    let mut before = RECT::default();
    unsafe { GetWindowRect(window.0, &mut before)? };
    WindowShowRequest {
        initial_bounds: None,
        ..request
    }
    .initialize()?;
    let mut after = RECT::default();
    unsafe { GetWindowRect(window.0, &mut after)? };
    assert_eq!(before, after, "later wakes must preserve resized geometry");
    assert!(!unsafe { IsWindowVisible(window.0) }.as_bool());
    Ok(())
}
