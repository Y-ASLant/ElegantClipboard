use gpui_kit::Window;
use objc2::{class, msg_send, runtime::AnyObject};
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

/// Configure only the menu host, outside GPUI borrows and before it is shown.
pub(crate) fn prepare_menu_window(native: isize) {
    let native = native as *mut AnyObject;
    unsafe {
        // GPUI's titlebar: None still uses Titled | FullSizeContentView.
        // Borderless is zero; NonactivatingPanel (1 << 7) keeps popup focus behavior.
        const NONACTIVATING_PANEL: usize = 1 << 7;
        let _: () = msg_send![native, setStyleMask: NONACTIVATING_PANEL];
        let _: () = msg_send![native, setHasShadow: false];
        let _: () = msg_send![native, setOpaque: false];
        let clear: *mut AnyObject = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![native, setBackgroundColor: clear];
    }
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

/// The non-activating panel already takes key focus when shown.
pub fn activate_for_input(window: &Window) {
    window.activate_window();
}

const NORMAL_WINDOW_LEVEL: isize = 0;
const FLOATING_WINDOW_LEVEL: isize = 3;
/// Matches `NSPopUpWindowLevel`, the level GPUI assigns to `WindowKind::PopUp`.
const POPUP_WINDOW_LEVEL: isize = 101;

/// Keep a normal window above other apps, or return it to the normal level.
pub fn set_above(window: &Window, above: bool) {
    set_level(
        window,
        if above {
            FLOATING_WINDOW_LEVEL
        } else {
            NORMAL_WINDOW_LEVEL
        },
    );
}

/// Drop the popup band so a normal editor can appear above the history panel.
pub fn release_above(window: &Window) {
    set_above(window, false);
}

/// Restore the floating panel level used when the history window was created.
pub fn raise_above(window: &Window) {
    set_level(window, POPUP_WINDOW_LEVEL);
}

/// Order the window in front without changing its level.
pub fn raise_front(window: &Window) {
    let Some(native) = window_id(window) else {
        return;
    };
    let native = native as *mut AnyObject;
    // SAFETY: native is the live NSWindow for this GPUI window.
    unsafe {
        let _: () = msg_send![native, orderFrontRegardless];
    }
}

fn set_level(window: &Window, level: isize) {
    if let Some(native) = window_id(window) {
        unsafe {
            let _: () = msg_send![native as *mut AnyObject, setLevel: level];
        }
    }
}
