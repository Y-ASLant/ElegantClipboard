//! AppKit owns pasteboard transfer and drag tracking; no general pasteboard writes.
use super::{cocoa, drag_payload::PreparedExternalDrag, external_drag::*};
use anyhow::{Context, Result, bail};
use objc2::{
    class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, AnyProtocol, ClassBuilder, Sel},
    sel,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize, NSString};
use std::{cell::Cell, sync::LazyLock};

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static RESULT: Cell<Option<(NSPoint, usize)>> = const { Cell::new(None) };
    static ESCAPED: Cell<bool> = const { Cell::new(false) };
}

extern "C-unwind" fn operation(
    _this: &AnyObject,
    _cmd: Sel,
    _session: *mut AnyObject,
    context: isize,
) -> usize {
    // Never offer an AppKit self-drop; GPUI commits the internal move instead.
    if context == 0 { 1 } else { 0 }
}

extern "C-unwind" fn ended(
    _this: &AnyObject,
    _cmd: Sel,
    _session: *mut AnyObject,
    point: NSPoint,
    operation: usize,
) {
    // AppKit may consume Escape inside its own tracking loop before sendEvent.
    if drag_escape_pressed() {
        ESCAPED.set(true);
    }
    RESULT.set(Some((point, operation)));
}

fn source_class() -> &'static AnyClass {
    static CLASS: LazyLock<&'static AnyClass> = LazyLock::new(|| {
        let mut class = ClassBuilder::new(c"ElegantClipboardDraggingSource", class!(NSObject))
            .expect("unique drag source class");
        if let Some(protocol) = AnyProtocol::get(c"NSDraggingSource") {
            class.add_protocol(protocol);
        }
        unsafe {
            class.add_method(
                sel!(draggingSession:sourceOperationMaskForDraggingContext:),
                operation as extern "C-unwind" fn(_, _, _, _) -> _,
            );
            class.add_method(
                sel!(draggingSession:endedAtPoint:operation:),
                ended as extern "C-unwind" fn(_, _, _, _, _),
            );
        }
        class.register()
    });
    &CLASS
}

struct ActiveDrag;
impl Drop for ActiveDrag {
    fn drop(&mut self) {
        ACTIVE.set(false);
        RESULT.set(None);
        ESCAPED.set(false);
    }
}

fn dragging_items(
    payload: &PreparedExternalDrag,
    position: NSPoint,
) -> Result<Retained<AnyObject>> {
    let items: Retained<AnyObject> =
        unsafe { msg_send![class!(NSMutableArray), arrayWithCapacity: payload.0.len()] };
    for item in payload.0.iter() {
        let writer: Retained<AnyObject> = unsafe { msg_send![class!(NSPasteboardItem), new] };
        if let Some(path) = &item.path {
            let url: Retained<AnyObject> =
                unsafe { msg_send![class!(NSURL), fileURLWithPath: &*NSString::from_str(path)] };
            let url_string: Retained<NSString> = unsafe { msg_send![&url, absoluteString] };
            let written: bool = unsafe {
                msg_send![&writer, setString: &*url_string, forType: &*NSString::from_str("public.file-url")]
            };
            if !written {
                bail!("无法准备 macOS 文件拖放数据")
            }
        }
        for (format, bytes) in &item.formats {
            let data = NSData::with_bytes(bytes);
            let written: bool = unsafe {
                msg_send![&writer, setData: &*data, forType: &*NSString::from_str(format)]
            };
            if !written {
                bail!("无法准备 macOS 拖放格式 {format}")
            }
        }
        let dragging: Retained<AnyObject> = unsafe {
            msg_send![msg_send![class!(NSDraggingItem), alloc], initWithPasteboardWriter: &*writer]
        };
        let image: Retained<AnyObject> = if let Some(path) = &item.path {
            unsafe { msg_send![&cocoa::workspace(), iconForFile: &*NSString::from_str(path)] }
        } else {
            unsafe {
                msg_send![class!(NSImage), imageNamed: &*NSString::from_str("NSMultipleDocuments")]
            }
        };
        unsafe {
            let _: () = msg_send![&dragging, setDraggingFrame: NSRect::new(position, NSSize::new(32.0, 32.0)), contents: &*image];
            let _: () = msg_send![&items, addObject: &*dragging];
        }
    }
    Ok(items)
}

pub fn run_external_drag(
    payload: PreparedExternalDrag,
    options: ExternalDragOptions,
) -> Result<ExternalDragOutcome> {
    let window = cocoa::window(options.source_window)?;
    if ACTIVE.replace(true) {
        bail!("macOS 拖放会话已在运行")
    }
    let _active = ActiveDrag;
    RESULT.set(None);
    ESCAPED.set(false);
    let application: Retained<AnyObject> =
        unsafe { msg_send![class!(NSApplication), sharedApplication] };
    let pointer = poll_drag_pointer()?;
    if !pointer.left_button_down {
        return Ok(ExternalDragOutcome::Cancelled);
    }
    let view: Retained<AnyObject> = unsafe { msg_send![&window, contentView] };
    let position: NSPoint = unsafe {
        msg_send![&window, convertPointFromScreen: cocoa::cocoa_point(pointer.x, pointer.y)]
    };
    // Payload preparation is asynchronous, so NSApplication.currentEvent may already
    // refer to a different event. Reconstruct tracking from the still-held real pointer.
    let process: Retained<AnyObject> = unsafe { msg_send![class!(NSProcessInfo), processInfo] };
    let timestamp: f64 = unsafe { msg_send![&process, systemUptime] };
    let number: isize = unsafe { msg_send![&window, windowNumber] };
    let modifiers: usize = unsafe { msg_send![class!(NSEvent), modifierFlags] };
    let event: Option<Retained<AnyObject>> = unsafe {
        msg_send![class!(NSEvent), mouseEventWithType: 6usize, location: position,
            modifierFlags: modifiers, timestamp: timestamp, windowNumber: number,
            context: std::ptr::null::<AnyObject>(), eventNumber: 0isize,
            clickCount: 1isize, pressure: 1.0f32]
    };
    let event = event.context("无法创建 macOS 鼠标拖动事件")?;
    let position: NSPoint = unsafe {
        msg_send![&view, convertPoint: position, fromView: std::ptr::null::<AnyObject>()]
    };
    let items = dragging_items(&payload, position)?;
    let source: Retained<AnyObject> = unsafe { msg_send![source_class(), new] };
    if let Some(preview) = options.preview_window {
        hide_drag_preview(preview)?;
    }
    let session: Option<Retained<AnyObject>> = unsafe {
        msg_send![&view, beginDraggingSessionWithItems: &*items, event: &*event, source: &*source]
    };
    let _session = session.context("macOS 拖放会话启动失败")?;
    // The caller deliberately releases all GPUI borrows before this nested AppKit loop.
    let mode = NSString::from_str("NSEventTrackingRunLoopMode");
    while RESULT.get().is_none() {
        objc2::rc::autoreleasepool(|_| {
            let deadline: Retained<AnyObject> =
                unsafe { msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.02f64] };
            let event: Option<Retained<AnyObject>> = unsafe {
                msg_send![&application, nextEventMatchingMask: usize::MAX, untilDate: &*deadline, inMode: &*mode, dequeue: true]
            };
            if let Some(event) = event {
                let kind: usize = unsafe { msg_send![&event, type] };
                if kind == 10 {
                    let key: u16 = unsafe { msg_send![&event, keyCode] };
                    if key == 53 {
                        ESCAPED.set(true);
                    }
                }
                unsafe {
                    let _: () = msg_send![&application, sendEvent: &*event];
                }
            }
        });
    }
    let (point, operation) = RESULT.get().expect("completed drag");
    if operation != 0 {
        return Ok(ExternalDragOutcome::Dropped);
    }
    let pointer = DragPointerState {
        x: point.x as i32,
        y: (cocoa::screen_height() - point.y) as i32,
        left_button_down: false,
    };
    if !ESCAPED.get() && !pointer_outside_source(options.source_window, pointer)? {
        Ok(ExternalDragOutcome::ReleasedInSource)
    } else {
        Ok(ExternalDragOutcome::Cancelled)
    }
}
