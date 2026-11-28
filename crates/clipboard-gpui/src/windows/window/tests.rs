use super::{
    WindowShowRequest, activate_hwnd_for_input, bounds_to_physical_rect, handle_mouse_activate,
    is_click_suppression_enabled, point_in_rect, set_click_activate_region, set_noactivate_style,
    suppress_click_activation,
};
use gpui_kit::{Bounds, point, px, size};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::ClientToScreen,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GetClientRect, GetWindowLongPtrW,
            GetWindowRect, IsWindowVisible, MA_ACTIVATE, MA_NOACTIVATE, SendMessageW, WA_ACTIVE,
            WA_INACTIVE, WINDOW_EX_STYLE, WM_ACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOPMOST,
            WS_OVERLAPPEDWINDOW,
        },
    },
    core::w,
};

struct HiddenWindow(HWND);

impl HiddenWindow {
    fn new(title: windows::core::PCWSTR) -> anyhow::Result<Self> {
        Self::with_style(title, WINDOW_EX_STYLE::default())
    }

    fn with_style(title: windows::core::PCWSTR, style: WINDOW_EX_STYLE) -> anyhow::Result<Self> {
        let hwnd = unsafe {
            CreateWindowExW(
                style,
                w!("STATIC"),
                title,
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
        };
        Ok(Self(hwnd))
    }

    fn has_ex_style(&self, flag: WINDOW_EX_STYLE) -> bool {
        (unsafe { GetWindowLongPtrW(self.0, GWL_EXSTYLE) } & flag.0 as isize) != 0
    }
}

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
    let window = HiddenWindow::new(w!("Isolated hidden placement test"))?;
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

#[test]
fn click_activate_bounds_convert_to_physical_rect_and_hit_test_half_open() {
    let rect = bounds_to_physical_rect(
        Bounds::new(point(px(12.5), px(30.2)), size(px(200.4), px(36.0))),
        1.25,
    )
    .expect("positive scaled bounds produce a physical rect");
    assert_eq!(
        rect,
        RECT {
            left: 15,
            top: 37,
            right: 267,
            bottom: 83,
        }
    );
    assert!(point_in_rect(POINT { x: 15, y: 37 }, rect));
    assert!(point_in_rect(POINT { x: 266, y: 82 }, rect));
    assert!(!point_in_rect(POINT { x: 14, y: 37 }, rect));
    assert!(!point_in_rect(POINT { x: 267, y: 82 }, rect));
    assert!(!point_in_rect(POINT { x: 20, y: 83 }, rect));

    assert_eq!(
        bounds_to_physical_rect(
            Bounds::new(point(px(10.), px(10.)), size(px(0.), px(30.))),
            1.0
        ),
        None
    );
    assert_eq!(
        bounds_to_physical_rect(
            Bounds::new(point(px(10.), px(10.)), size(px(100.), px(30.))),
            0.0
        ),
        None
    );
}

#[test]
fn mouse_activate_toggles_noactivate_only_inside_registered_search_rect() -> anyhow::Result<()> {
    let window = HiddenWindow::new(w!("Isolated click activate region test"))?;
    let handle = window.0.0 as isize;
    suppress_click_activation(handle)?;
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));

    let search_bounds = Bounds::new(point(px(12.), px(30.)), size(px(168.), px(36.)));
    set_click_activate_region(handle, Some(search_bounds), 1.0);

    // Click inside search region while an external app is foreground: allow activation.
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 45 }),
            Some(1.0),
            false,
            false
        ),
        Some(LRESULT(MA_ACTIVATE as isize))
    );
    assert!(!window.has_ex_style(WS_EX_NOACTIVATE));

    // Click outside search region while an external app is foreground: suppress activation.
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 120 }),
            Some(1.0),
            false,
            false
        ),
        Some(LRESULT(MA_NOACTIVATE as isize))
    );
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));

    // Clearing bounds (onboarding / legacy migration) keeps every click non-activating.
    set_click_activate_region(handle, None, 1.0);
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 45 }),
            Some(1.0),
            false,
            false
        ),
        Some(LRESULT(MA_NOACTIVATE as isize))
    );
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));

    // When another window of our process owns foreground, clicking search still activates.
    set_click_activate_region(handle, Some(search_bounds), 1.0);
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 45 }),
            Some(1.0),
            false,
            true
        ),
        Some(LRESULT(MA_ACTIVATE as isize))
    );
    assert!(!window.has_ex_style(WS_EX_NOACTIVATE));
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 120 }),
            Some(1.0),
            false,
            true
        ),
        None
    );

    // When this window itself is already foreground, let DefSubclassProc handle it.
    assert_eq!(
        handle_mouse_activate(window.0, None, None, true, true),
        None
    );
    Ok(())
}

#[test]
fn mouse_activate_rescales_logical_search_bounds_across_dpi_changes() -> anyhow::Result<()> {
    let window = HiddenWindow::new(w!("Isolated cross-DPI click region test"))?;
    let handle = window.0.0 as isize;
    suppress_click_activation(handle)?;

    // Registered on a 150% display: logical [12..212, 30..66].
    let search_bounds = Bounds::new(point(px(12.), px(30.)), size(px(200.), px(36.)));
    set_click_activate_region(handle, Some(search_bounds), 1.5);

    // Moved to a 100% display without re-running cached prepaint: physical (40, 80)
    // would have hit the stale 150% rect [45..99), but at 100% it is on a card below [30..66).
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 80 }),
            Some(1.0),
            false,
            false
        ),
        Some(LRESULT(MA_NOACTIVATE as isize))
    );
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 40, y: 45 }),
            Some(1.0),
            false,
            false
        ),
        Some(LRESULT(MA_ACTIVATE as isize))
    );

    // Registered on 100%, moved to 150%: right side of the search box [18..318, 45..99) hits.
    set_click_activate_region(handle, Some(search_bounds), 1.0);
    assert_eq!(
        handle_mouse_activate(
            window.0,
            Some(POINT { x: 260, y: 60 }),
            Some(1.5),
            false,
            false
        ),
        Some(LRESULT(MA_ACTIVATE as isize))
    );

    // Unavailable live DPI (None) falls back to the registered scale (1.0 -> [12..212, 30..66)).
    assert_eq!(
        handle_mouse_activate(window.0, Some(POINT { x: 260, y: 60 }), None, false, false),
        Some(LRESULT(MA_NOACTIVATE as isize))
    );
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));
    Ok(())
}

#[test]
fn failed_activate_for_input_restores_noactivate_only_for_suppressed_windows() -> anyhow::Result<()>
{
    // Establish the starting style at creation; the test is about activation
    // rollback, not whether a hidden window can be reordered on the CI desktop.
    let suppressed = HiddenWindow::with_style(
        w!("Isolated failed activation suppressed test"),
        WS_EX_TOPMOST,
    )?;
    suppress_click_activation(suppressed.0.0 as isize)?;
    assert!(suppressed.has_ex_style(WS_EX_NOACTIVATE));
    assert!(suppressed.has_ex_style(WS_EX_TOPMOST));

    // A hidden window cannot take foreground, exercising the denied-activation rollback path.
    activate_hwnd_for_input(suppressed.0);
    assert!(
        suppressed.has_ex_style(WS_EX_NOACTIVATE),
        "failed activation must restore WS_EX_NOACTIVATE on click-suppressed panels"
    );
    assert!(
        suppressed.has_ex_style(WS_EX_TOPMOST),
        "failed activation must not drop HWND_TOPMOST"
    );

    let normal = HiddenWindow::new(w!("Isolated failed activation normal test"))?;
    assert!(!normal.has_ex_style(WS_EX_NOACTIVATE));
    activate_hwnd_for_input(normal.0);
    assert!(
        !normal.has_ex_style(WS_EX_NOACTIVATE),
        "failed activation must not add WS_EX_NOACTIVATE to normal windows"
    );
    Ok(())
}

#[test]
fn activate_deactivate_reshow_and_destroy_maintain_noactivate_and_topmost_state()
-> anyhow::Result<()> {
    let window = HiddenWindow::with_style(w!("Isolated activation lifecycle test"), WS_EX_TOPMOST)?;
    let handle = window.0.0 as isize;
    suppress_click_activation(handle)?;
    assert!(is_click_suppression_enabled(handle));
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));

    // Simulate topmost floating panel becoming active: topmost band must drop.
    assert!(window.has_ex_style(WS_EX_TOPMOST));
    set_noactivate_style(window.0, false);
    assert!(!window.has_ex_style(WS_EX_NOACTIVATE));

    unsafe {
        SendMessageW(
            window.0,
            WM_ACTIVATE,
            Some(WPARAM(WA_ACTIVE as usize)),
            Some(LPARAM(0)),
        );
    }
    assert!(
        !window.has_ex_style(WS_EX_TOPMOST),
        "activating the panel must drop HWND_TOPMOST so dialogs can stack above it"
    );

    // Deactivation restores WS_EX_NOACTIVATE so a later card click cannot steal focus.
    unsafe {
        SendMessageW(
            window.0,
            WM_ACTIVATE,
            Some(WPARAM(WA_INACTIVE as usize)),
            Some(LPARAM(0)),
        );
    }
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));

    // Re-showing via WindowShowRequest::show must also restore WS_EX_NOACTIVATE and HWND_TOPMOST.
    set_noactivate_style(window.0, false);
    assert!(!window.has_ex_style(WS_EX_NOACTIVATE));
    WindowShowRequest {
        hwnd: window.0,
        initial_bounds: None,
        scale: 1.0,
    }
    .show();
    assert!(window.has_ex_style(WS_EX_NOACTIVATE));
    assert!(window.has_ex_style(WS_EX_TOPMOST));

    // Destroying the window clears the click-suppression state entry.
    drop(window);
    assert!(!is_click_suppression_enabled(handle));
    Ok(())
}
