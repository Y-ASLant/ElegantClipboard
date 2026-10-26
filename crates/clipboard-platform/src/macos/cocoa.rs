use anyhow::{Result, bail};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSPoint, NSRect};

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

pub(crate) fn main_thread() -> Result<()> {
    let is_main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !is_main {
        bail!("macOS 窗口操作必须在主线程执行");
    }
    Ok(())
}

pub(crate) fn workspace() -> Retained<AnyObject> {
    unsafe { msg_send![class!(NSWorkspace), sharedWorkspace] }
}

pub(crate) fn window(handle: isize) -> Result<Retained<AnyObject>> {
    main_thread()?;
    // Handles originate from GPUI's AppKit NSView.window and remain owned by GPUI.
    let object = unsafe { Retained::retain(handle as *mut AnyObject) };
    let Some(object) = object else {
        bail!("macOS 窗口句柄为空")
    };
    let valid: bool = unsafe { msg_send![&object, isKindOfClass: class!(NSWindow)] };
    if !valid {
        bail!("macOS 窗口句柄不是 NSWindow")
    }
    Ok(object)
}

pub(crate) fn screen_height() -> f64 {
    let screens: Retained<AnyObject> = unsafe { msg_send![class!(NSScreen), screens] };
    let first: Retained<AnyObject> = unsafe { msg_send![&screens, objectAtIndex: 0usize] };
    let frame: NSRect = unsafe { msg_send![&first, frame] };
    frame.size.height
}

pub(crate) fn cocoa_point(x: i32, y: i32) -> NSPoint {
    NSPoint::new(f64::from(x), screen_height() - f64::from(y))
}
