use std::time::{Duration, Instant};

use ::windows::Win32::{
    Foundation::HWND,
    System::DataExchange::GetClipboardSequenceNumber,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
            VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
        },
    },
};
use anyhow::{Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use clipboard_platform::paste_delivery::{
    ReadyResult, paste_shortcut_trigger_held, PasteWaitPolicy,
};
use gpui_kit::Window;

pub const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub fn is_external_target(window: &Window, target: (isize, u32)) -> bool {
    if !is_same_window(target) || target.1 == std::process::id() {
        return false;
    }
    super::window::window_hwnd(window).is_some_and(|own| own.0 as isize != target.0)
}

pub struct PendingPaste {
    target: (isize, u32),
    clipboard_sequence: u32,
    paste_key: PasteKeyPreference,
    wait: PasteWaitPolicy,
    started: Instant,
}

struct TargetState {
    same_window: bool,
    foreground: bool,
    clipboard_sequence: u32,
    modifiers_down: bool,
}

impl PendingPaste {
    pub fn begin(
        target: (isize, u32),
        clipboard_sequence: u32,
        paste_key: PasteKeyPreference,
        wait: PasteWaitPolicy,
    ) -> Result<Self> {
        if !is_same_window(target) || target.1 == std::process::id() {
            bail!("目标窗口已关闭，内容已复制，请手动粘贴");
        }
        let hwnd = HWND(target.0 as *mut _);
        let foreground = unsafe { GetForegroundWindow() };
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(foreground, Some(&mut process_id)) };
        if foreground != hwnd && process_id != std::process::id() {
            bail!("前台窗口已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        if clipboard_sequence == 0 || unsafe { GetClipboardSequenceNumber() } != clipboard_sequence
        {
            bail!("剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        // Focus only once. Retrying focus while waiting could override a user's app switch.
        if !unsafe { SetForegroundWindow(hwnd) }.as_bool()
            || unsafe { GetForegroundWindow() } != hwnd
        {
            bail!("无法聚焦原窗口，内容已复制，请手动粘贴");
        }
        Ok(Self {
            target,
            clipboard_sequence,
            paste_key,
            wait,
            started: Instant::now(),
        })
    }

    /// Returns false while the user's modifiers are held; never releases them.
    pub fn try_send(&self) -> Result<bool> {
        let state = TargetState {
            same_window: is_same_window(self.target),
            foreground: unsafe { GetForegroundWindow() }.0 as isize == self.target.0,
            clipboard_sequence: unsafe { GetClipboardSequenceNumber() },
            modifiers_down: [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
                .into_iter()
                .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0),
        };
        if !self.ready(state)? {
            return Ok(false);
        }
        let inputs = paste_inputs(self.paste_key);
        let size = std::mem::size_of::<INPUT>() as i32;
        let sent = unsafe { SendInput(&inputs[..2], size) };
        if sent != 2 {
            let _ = unsafe { SendInput(&inputs[2..], size) };
            bail!("目标窗口未接受粘贴快捷键，内容已复制，请手动粘贴");
        }
        std::thread::sleep(Duration::from_millis(8));
        let sent = unsafe { SendInput(&inputs[2..], size) };
        if sent != 2 {
            bail!("目标窗口未接受粘贴快捷键，内容已复制，请手动粘贴");
        }
        Ok(true)
    }

    fn ready(&self, state: TargetState) -> Result<bool> {
        if !state.same_window {
            bail!("目标窗口已关闭，内容已复制，请手动粘贴");
        }
        if !state.foreground {
            bail!("前台窗口已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        if self.clipboard_sequence == 0 || state.clipboard_sequence != self.clipboard_sequence {
            bail!("剪贴板内容已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        let trigger_held = self
            .wait
            .trigger_shortcut
            .as_deref()
            .is_some_and(paste_shortcut_trigger_held);
        match self.wait.ready_to_inject(
            state.modifiers_down,
            trigger_held,
            self.started.elapsed(),
        ) {
            ReadyResult::Ready => Ok(true),
            ReadyResult::Waiting => Ok(false),
            ReadyResult::TimedOut => {
                bail!("等待修饰键释放超时，已取消自动粘贴；请松开按键后手动粘贴")
            }
        }
    }
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
    let scan = unsafe { MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = KEYEVENTF_SCANCODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if key == VK_INSERT {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS;
    use clipboard_platform::paste_delivery::PASTE_INJECT_WAIT;

    fn pending() -> PendingPaste {
        PendingPaste {
            target: (1, 1),
            clipboard_sequence: 7,
            paste_key: PasteKeyPreference::CtrlV,
            wait: PasteWaitPolicy::controlled(),
            started: Instant::now(),
        }
    }

    fn state(modifiers_down: bool) -> TargetState {
        TargetState {
            same_window: true,
            foreground: true,
            clipboard_sequence: 7,
            modifiers_down,
        }
    }

    #[test]
    fn waits_for_release_only_until_deadline() {
        let mut paste = pending();
        paste.started = Instant::now() - (PASTE_INJECT_WAIT - Duration::from_millis(1));
        assert!(!paste.ready(state(true)).unwrap());
        assert!(paste.ready(state(false)).unwrap());
        paste.started = Instant::now() - PASTE_INJECT_WAIT;
        for modifiers_down in [true, false] {
            assert!(paste.ready(state(modifiers_down)).is_err());
        }
    }

    #[test]
    fn target_or_clipboard_changes_cancel_even_while_modifiers_are_held() {
        let paste = pending();
        for modifiers_down in [true, false] {
            for changed in [
                TargetState {
                    same_window: false,
                    ..state(modifiers_down)
                },
                TargetState {
                    foreground: false,
                    ..state(modifiers_down)
                },
                TargetState {
                    clipboard_sequence: 8,
                    ..state(modifiers_down)
                },
            ] {
                assert!(paste.ready(changed).is_err());
            }
        }
        let mut unknown_sequence = pending();
        unknown_sequence.clipboard_sequence = 0;
        let mut snapshot = state(false);
        snapshot.clipboard_sequence = 0;
        assert!(unknown_sequence.ready(snapshot).is_err());
    }

    #[test]
    fn paste_key_setting_selects_the_expected_key_sequence() {
        for (preference, modifier, key) in [
            (PasteKeyPreference::CtrlV, VK_CONTROL, VK_V),
            (PasteKeyPreference::ShiftInsert, VK_SHIFT, VK_INSERT),
        ] {
            let inputs = paste_inputs(preference);
            let actual = inputs.map(|input| unsafe { input.Anonymous.ki });
            assert_eq!(actual.map(|input| input.wVk), [VIRTUAL_KEY(0); 4]);
            let scan = |key: VIRTUAL_KEY| unsafe {
                MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC) as u16
            };
            assert_eq!(
                actual.map(|input| input.wScan),
                [scan(modifier), scan(key), scan(key), scan(modifier)]
            );
            let extended = if key == VK_INSERT {
                KEYEVENTF_EXTENDEDKEY
            } else {
                KEYBD_EVENT_FLAGS(0)
            };
            assert_eq!(actual[0].dwFlags, KEYEVENTF_SCANCODE);
            assert_eq!(actual[1].dwFlags, KEYEVENTF_SCANCODE | extended);
            assert_eq!(
                actual[2].dwFlags,
                KEYEVENTF_SCANCODE | extended | KEYEVENTF_KEYUP
            );
            assert_eq!(actual[3].dwFlags, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP);
        }
    }
}
