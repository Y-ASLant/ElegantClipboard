use ::windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetDoubleClickTime, VK_SHIFT,
};
use std::time::Duration;

pub(crate) fn double_click_interval() -> Duration {
    Duration::from_millis(u64::from(unsafe { GetDoubleClickTime() }))
}

pub(crate) fn shift_pressed() -> bool {
    unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) < 0 }
}
