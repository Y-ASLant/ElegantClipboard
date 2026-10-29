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

/// Read accessibility preferences without changing any system setting.
pub(crate) fn client_area_animations_enabled() -> ::windows::core::Result<bool> {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut enabled = ::windows::core::BOOL::default();
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL into this live local.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut ::windows::core::BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )?;
    }
    Ok(enabled.as_bool())
}
