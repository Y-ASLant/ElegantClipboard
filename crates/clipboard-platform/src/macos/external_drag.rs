use super::cocoa;
use anyhow::{Result, bail};
use objc2::{class, msg_send};
use objc2_foundation::{NSPoint, NSRect, NSSize};

pub use super::drag_payload::{PreparedExternalDrag, prepare_external_drag};
pub use super::drag_session::run_external_drag;

// Quartz pointer coordinates are logical points, origin at main-screen top left.
pub use crate::drag_types::{DragPointerState, ExternalDragOptions, ExternalDragOutcome};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
}

pub fn poll_drag_pointer() -> Result<DragPointerState> {
    cocoa::main_thread()?;
    let position: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    let buttons: usize = unsafe { msg_send![class!(NSEvent), pressedMouseButtons] };
    Ok(DragPointerState {
        x: position.x as i32,
        y: (cocoa::screen_height() - position.y) as i32,
        left_button_down: buttons & 1 != 0,
    })
}

pub fn drag_escape_pressed() -> bool {
    unsafe { CGEventSourceKeyState(1, 53) }
}

pub fn drag_pointer_in_client(
    handle: isize,
    pointer: DragPointerState,
    scale_factor: f32,
) -> Result<(f32, f32)> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        bail!("拖放窗口缩放比例无效")
    }
    let window = cocoa::window(handle)?;
    let view: objc2::rc::Retained<objc2::runtime::AnyObject> =
        unsafe { msg_send![&window, contentView] };
    let point: NSPoint = unsafe {
        msg_send![&window, convertPointFromScreen: cocoa::cocoa_point(pointer.x, pointer.y)]
    };
    let point: NSPoint = unsafe {
        msg_send![&view, convertPoint: point, fromView: std::ptr::null::<objc2::runtime::AnyObject>()]
    };
    let bounds: NSRect = unsafe { msg_send![&view, bounds] };
    let flipped: bool = unsafe { msg_send![&view, isFlipped] };
    Ok((
        point.x as f32,
        if flipped {
            point.y
        } else {
            bounds.size.height - point.y
        } as f32,
    ))
}

pub fn pointer_outside_source(handle: isize, pointer: DragPointerState) -> Result<bool> {
    let window = cocoa::window(handle)?;
    let frame: NSRect = unsafe { msg_send![&window, frame] };
    let point = cocoa::cocoa_point(pointer.x, pointer.y);
    Ok(point.x < frame.origin.x
        || point.x >= frame.origin.x + frame.size.width
        || point.y < frame.origin.y
        || point.y >= frame.origin.y + frame.size.height)
}

pub fn configure_drag_preview(handle: isize) -> Result<()> {
    let window = cocoa::window(handle)?;
    unsafe {
        let _: () = msg_send![&window, setIgnoresMouseEvents: true];
        let _: () = msg_send![&window, setLevel: 3isize];
        let _: () = msg_send![&window, setHasShadow: false];
    }
    Ok(())
}

pub fn move_drag_preview(handle: isize, x: i32, y: i32) -> Result<()> {
    let window = cocoa::window(handle)?;
    unsafe {
        let _: () = msg_send![&window, setFrameTopLeftPoint: cocoa::cocoa_point(x, y)];
    }
    Ok(())
}

pub fn show_drag_preview(handle: isize) -> Result<()> {
    let window = cocoa::window(handle)?;
    unsafe {
        let _: () = msg_send![&window, orderFrontRegardless];
    }
    Ok(())
}

pub fn hide_drag_preview(handle: isize) -> Result<()> {
    let window = cocoa::window(handle)?;
    unsafe {
        let _: () = msg_send![&window, orderOut: std::ptr::null::<objc2::runtime::AnyObject>()];
    }
    Ok(())
}

pub fn drag_preview_scale(handle: isize) -> Result<f32> {
    cocoa::window(handle)?;
    // Both pointer coordinates and NSWindow geometry use points, including Retina.
    Ok(1.0)
}

pub fn resize_drag_preview(handle: isize, width: i32, height: i32) -> Result<()> {
    if width <= 0 || height <= 0 {
        bail!("拖放预览窗口尺寸无效")
    }
    let window = cocoa::window(handle)?;
    unsafe {
        let _: () =
            msg_send![&window, setContentSize: NSSize::new(f64::from(width), f64::from(height))];
    }
    Ok(())
}
