use std::time::Duration;

pub fn shift_pressed() -> bool {
    clipboard_platform::desktop::shift_pressed()
}

#[cfg(target_os = "linux")]
pub fn double_click_interval() -> Duration {
    use gtk::prelude::ObjectExt;
    let millis = gtk::Settings::default()
        .map(|settings| settings.property::<i32>("gtk-double-click-time"))
        .unwrap_or(400);
    Duration::from_millis(millis.max(1) as u64)
}

#[cfg(target_os = "macos")]
pub fn double_click_interval() -> Duration {
    let seconds: f64 = unsafe { objc2::msg_send![objc2::class!(NSEvent), doubleClickInterval] };
    Duration::from_secs_f64(seconds.max(0.001))
}
