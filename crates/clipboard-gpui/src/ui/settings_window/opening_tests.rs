use crate::{options::Options, ui};
use std::cell::Cell;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        CBT_CREATEWNDW, CallNextHookEx, HCBT_CREATEWND, IsWindowVisible, KillTimer, SetTimer,
        SetWindowsHookExW, UnhookWindowsHookEx, WH_CBT, WS_THICKFRAME,
    },
};

thread_local! {
    static MAIN: Cell<HWND> = Cell::default();
    static AT_REJECTION: Cell<Option<bool>> = const { Cell::new(None) };
    static AFTER_REJECTION: Cell<Option<bool>> = const { Cell::new(None) };
}

unsafe extern "system" fn sample_after_open(_: HWND, _: u32, timer: usize, _: u32) {
    unsafe {
        let _ = KillTimer(None, timer);
        AFTER_REJECTION.set(Some(IsWindowVisible(MAIN.get()).as_bool()));
    }
}

unsafe extern "system" fn reject_settings(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_CREATEWND as i32 {
        // HCBT_CREATEWND provides these two structures for the duration of the hook.
        let create = unsafe { &*(*(lparam.0 as *const CBT_CREATEWNDW)).lpcs };
        let name = create.lpszName;
        if create.style as u32 & WS_THICKFRAME.0 != 0 && name.0 as usize > u16::MAX as usize {
            // A name that is neither null nor a resource ID is a NUL-terminated UTF-16 title.
            let title = unsafe { name.as_wide() };
            let named = |s: &str| title.iter().copied().eq(s.encode_utf16());
            if named("ElegantClipboard") {
                MAIN.set(HWND(wparam.0 as _));
            } else if named("设置") || named("Settings") {
                unsafe {
                    AT_REJECTION.set(Some(IsWindowVisible(MAIN.get()).as_bool()));
                    // The thread dispatches this timer after GPUI returns from the failed open.
                    SetTimer(None, 0, 50, Some(sample_after_open));
                }
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[test]
#[ignore = "requires an isolated interactive Windows desktop; opens the GPUI smoke window"]
fn settings_creation_failure_keeps_main_visible() -> anyhow::Result<()> {
    let data = tempfile::tempdir()?;
    // Only this test's own UI thread is hooked. No foreign process or user data is involved.
    let hook =
        unsafe { SetWindowsHookExW(WH_CBT, Some(reject_settings), None, GetCurrentThreadId())? };
    let result = ui::run(
        Options {
            data_dir: Some(data.path().to_owned()),
            monitor: false,
            smoke_test: true,
            start_hidden: false,
            wait_for_instance: false,
        },
        None,
    );
    unsafe { UnhookWindowsHookEx(hook)? };
    result?;
    assert_eq!(
        AT_REJECTION.get(),
        Some(true),
        "main must remain visible while opening settings"
    );
    assert_eq!(
        AFTER_REJECTION.get(),
        Some(true),
        "creation failure must not hide main"
    );
    Ok(())
}
