//! Opt-in native regression: clicks must preserve an external input queue's focus.
//! Uses a fresh blank Notepad and a test-owned history window; never reads or
//! writes the clipboard or user data. Moves/restores the pointer and closes both
//! fixtures. Run only on an isolated interactive Windows desktop.

use super::{
    WindowShowRequest, activate_hwnd_for_input, live_window_scale, set_click_activate_region,
    suppress_click_activation,
};
use gpui_kit::{Bounds, point, px, size};
use std::{
    process::Child,
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::ClientToScreen,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
                SendInput,
            },
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
                GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
                GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible, MSG, PM_REMOVE,
                PeekMessageW, RegisterClassW, SWP_NOACTIVATE, SetCursorPos, SetWindowPos,
                TranslateMessage, WINDOW_EX_STYLE, WM_LBUTTONUP, WNDCLASSW, WS_EX_NOACTIVATE,
                WS_OVERLAPPEDWINDOW, WindowFromPoint,
            },
        },
    },
    core::{BOOL, w},
};

struct Fixtures {
    target: Child,
    history: HWND,
    pointer: POINT,
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.history);
            let _ = SetCursorPos(self.pointer.x, self.pointer.y);
        }
        let _ = self.target.kill();
        let _ = self.target.wait();
    }
}

struct TargetWindow {
    process_id: u32,
    hwnd: HWND,
}

unsafe extern "system" fn find_target(hwnd: HWND, data: LPARAM) -> BOOL {
    let target = unsafe { &mut *(data.0 as *mut TargetWindow) };
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id == target.process_id && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        target.hwnd = hwnd;
        return BOOL(0);
    }
    BOOL(1)
}

struct Click {
    received: bool,
    foreground: HWND,
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}

unsafe extern "system" fn observe_click(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    if message == WM_LBUTTONUP {
        let click = unsafe { &mut *(data as *mut Click) };
        click.received = true;
        click.foreground = unsafe { GetForegroundWindow() };
    }
    unsafe { DefSubclassProc(hwnd, message, w, l) }
}

fn pump_until(stage: &str, mut complete: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !complete() {
        assert!(
            Instant::now() < deadline,
            "native interaction timed out: {stage}"
        );
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn click_window(hwnd: HWND, point: POINT) -> anyhow::Result<()> {
    unsafe {
        SetCursorPos(point.x, point.y)?;
    }
    assert_eq!(
        unsafe { WindowFromPoint(point) },
        hwnd,
        "click is inside fixture"
    );
    let inputs = [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|dw_flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: dw_flags,
                ..Default::default()
            },
        },
    });
    assert_eq!(
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) },
        2
    );
    Ok(())
}

#[test]
#[ignore = "requires isolated interactive Windows desktop; opens blank Notepad and clicks a test window"]
fn history_click_preserves_external_foreground_and_allows_explicit_activation() -> anyhow::Result<()>
{
    let mut pointer = POINT::default();
    unsafe { GetCursorPos(&mut pointer)? };
    let mut fixtures = Fixtures {
        target: std::process::Command::new("notepad.exe").spawn()?,
        history: HWND::default(),
        pointer,
    };
    let mut target = TargetWindow {
        process_id: fixtures.target.id(),
        hwnd: HWND::default(),
    };
    pump_until("find blank Notepad", || {
        let _ = unsafe { EnumWindows(Some(find_target), LPARAM(&mut target as *mut _ as isize)) };
        !target.hwnd.is_invalid()
    });
    // A real click establishes foreground permission; SetForegroundWindow alone
    // can be denied when the shell running this opt-in test is not foreground.
    unsafe {
        SetWindowPos(
            target.hwnd,
            Some(HWND_TOPMOST),
            400,
            100,
            500,
            400,
            SWP_NOACTIVATE,
        )?;
    }
    click_window(target.hwnd, POINT { x: 460, y: 112 })?;
    pump_until(
        "Notepad foreground prerequisite",
        || unsafe { GetForegroundWindow() } == target.hwnd,
    );
    let class = w!("ElegantClipboard isolated click regression");
    assert_ne!(
        unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                lpszClassName: class,
                ..Default::default()
            })
        },
        0
    );
    fixtures.history = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Isolated history click test"),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            320,
            240,
            None,
            None,
            None,
            None,
        )?
    };
    suppress_click_activation(fixtures.history.0 as isize)?;
    let search_bounds = Bounds::new(point(px(16.), px(12.)), size(px(244.), px(36.)));
    let scale = live_window_scale(fixtures.history).unwrap_or(1.0);
    set_click_activate_region(fixtures.history.0 as isize, Some(search_bounds), scale);
    let mut click = Click {
        received: false,
        foreground: HWND::default(),
    };
    anyhow::ensure!(
        unsafe {
            SetWindowSubclass(
                fixtures.history,
                Some(observe_click),
                4,
                &mut click as *mut _ as usize,
            )
        }
        .as_bool(),
        "install click observer"
    );
    WindowShowRequest {
        hwnd: fixtures.history,
        initial_bounds: None,
        scale: 1.0,
    }
    .show();
    assert_eq!(
        unsafe { GetForegroundWindow() },
        target.hwnd,
        "show preserves target"
    );
    // Click outside the search box region: must preserve Notepad foreground.
    click_window(fixtures.history, POINT { x: 200, y: 200 })?;
    pump_until("mouse release received", || click.received);
    assert_eq!(
        click.foreground, target.hwnd,
        "click outside search must not clear or steal foreground"
    );
    assert_eq!(unsafe { GetForegroundWindow() }, target.hwnd);

    // Click inside the search box region: must activate the history window.
    let mut search_click = POINT {
        x: (40. * scale).round() as i32,
        y: (24. * scale).round() as i32,
    };
    unsafe { ClientToScreen(fixtures.history, &mut search_click).ok()? };
    click.received = false;
    click_window(fixtures.history, search_click)?;
    pump_until("search click activates history window", || {
        click.received && unsafe { GetForegroundWindow() } == fixtures.history
    });

    // Return foreground to Notepad, re-show history panel, and verify non-activating mode returns.
    click_window(target.hwnd, POINT { x: 460, y: 112 })?;
    pump_until(
        "Notepad regains foreground",
        || unsafe { GetForegroundWindow() } == target.hwnd,
    );
    WindowShowRequest {
        hwnd: fixtures.history,
        initial_bounds: None,
        scale: 1.0,
    }
    .show();
    assert_ne!(
        unsafe { GetWindowLongPtrW(fixtures.history, GWL_EXSTYLE) } & WS_EX_NOACTIVATE.0 as isize,
        0,
        "re-show restores WS_EX_NOACTIVATE"
    );
    click.received = false;
    click_window(fixtures.history, POINT { x: 200, y: 200 })?;
    pump_until("post-reshow card click received", || click.received);
    assert_eq!(unsafe { GetForegroundWindow() }, target.hwnd);

    // Explicit programmatic activation (typing / shortcut) must also take foreground.
    activate_hwnd_for_input(fixtures.history);
    pump_until(
        "intentional text activation",
        || unsafe { GetForegroundWindow() } == fixtures.history,
    );
    Ok(())
}
