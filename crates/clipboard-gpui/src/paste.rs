use anyhow::{Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::HWND,
    System::DataExchange::GetClipboardSequenceNumber,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU,
            VK_RWIN, VK_SHIFT, VK_V,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
        },
    },
};

pub fn is_external_target(window: &Window, target: (isize, u32)) -> bool {
    if !is_same_window(target) || target.1 == std::process::id() {
        return false;
    }
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    matches!(handle.as_raw(), RawWindowHandle::Win32(own) if own.hwnd.get() != target.0)
}

pub fn send_to_target(
    target: (isize, u32),
    clipboard_sequence: u32,
    paste_key: PasteKeyPreference,
) -> Result<()> {
    if !is_same_window(target) {
        bail!("目标窗口已关闭，内容已复制，请手动粘贴");
    }
    let target = HWND(target.0 as *mut _);
    if [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    {
        bail!("请先松开修饰键；内容已复制，可手动粘贴");
    }
    if !unsafe { SetForegroundWindow(target) }.as_bool()
        || unsafe { GetForegroundWindow() } != target
    {
        bail!("无法聚焦原窗口，内容已复制，请手动粘贴");
    }
    if clipboard_sequence == 0 || unsafe { GetClipboardSequenceNumber() } != clipboard_sequence {
        bail!("剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴");
    }
    let inputs = paste_inputs(paste_key);
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        let releases = [inputs[2], inputs[3]];
        unsafe { SendInput(&releases, std::mem::size_of::<INPUT>() as i32) };
        bail!("目标窗口未接受粘贴快捷键，内容已复制，请手动粘贴");
    }
    Ok(())
}

fn paste_inputs(paste_key: PasteKeyPreference) -> [INPUT; 4] {
    let (modifier, key) = match paste_key {
        PasteKeyPreference::CtrlV => (VK_CONTROL, VK_V),
        PasteKeyPreference::ShiftInsert => (VK_SHIFT, VK_INSERT),
    };
    [
        key_input(modifier, false),
        key_input(key, false),
        key_input(key, true),
        key_input(modifier, true),
    ]
}

fn is_same_window(target: (isize, u32)) -> bool {
    if target.0 == 0 || target.1 == 0 {
        return false;
    }
    let hwnd = HWND(target.0 as *mut _);
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return false;
    }
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id == target.1
}

fn key_input(key: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_key_setting_selects_the_expected_key_sequence() {
        for (preference, modifier, key) in [
            (PasteKeyPreference::CtrlV, VK_CONTROL, VK_V),
            (PasteKeyPreference::ShiftInsert, VK_SHIFT, VK_INSERT),
        ] {
            let inputs = paste_inputs(preference);
            let actual = inputs.map(|input| unsafe { input.Anonymous.ki });
            assert_eq!(
                actual.map(|input| input.wVk),
                [modifier, key, key, modifier]
            );
            assert_eq!(actual[0].dwFlags, KEYBD_EVENT_FLAGS(0));
            assert_eq!(actual[1].dwFlags, KEYBD_EVENT_FLAGS(0));
            assert_eq!(actual[2].dwFlags, KEYEVENTF_KEYUP);
            assert_eq!(actual[3].dwFlags, KEYEVENTF_KEYUP);
        }
    }
}
