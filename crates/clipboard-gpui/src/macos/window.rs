use gpui_kit::Window;
use objc2::{msg_send, runtime::AnyObject};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn window_id(window: &Window) -> Option<isize> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    let view = handle.ns_view.as_ptr().cast::<AnyObject>();
    let native: *mut AnyObject = unsafe { msg_send![view, window] };
    (!native.is_null()).then_some(native as isize)
}

pub fn set_window_visible(window: &Window, visible: bool) {
    if let Some(native) = window_id(window) {
        let native = native as *mut AnyObject;
        unsafe {
            if visible {
                let _: () = msg_send![native, deminiaturize: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![native, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
            } else {
                let _: () = msg_send![native, orderOut: std::ptr::null::<AnyObject>()];
            }
        }
        if visible {
            window.activate_window();
        }
    }
}

pub fn is_window_shown(window: &Window) -> bool {
    window_id(window)
        .is_some_and(|native| unsafe { msg_send![native as *mut AnyObject, isVisible] })
}

pub fn is_window_minimized(window: &Window) -> bool {
    window_id(window)
        .is_some_and(|native| unsafe { msg_send![native as *mut AnyObject, isMiniaturized] })
}
