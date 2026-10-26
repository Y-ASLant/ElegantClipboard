use super::cocoa;
use anyhow::{Context, Result};
use async_channel::Sender;
use block2::RcBlock;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSPoint;

pub struct OutsideClickMonitor {
    monitor: Retained<AnyObject>,
}

impl OutsideClickMonitor {
    pub fn start(events: Sender<(i32, i32)>, window_handle: isize) -> Result<Self> {
        let window = cocoa::window(window_handle)?;
        // NSEvent global monitors only receive events delivered to other applications.
        // Mouse monitoring does not require the keyboard Accessibility permission.
        let handler = RcBlock::new(move |_event: *mut AnyObject| {
            let visible: bool = unsafe { msg_send![&window, isVisible] };
            if !visible {
                return;
            }
            let point: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
            let _ = events.try_send((point.x as i32, (cocoa::screen_height() - point.y) as i32));
        });
        let monitor: Option<Retained<AnyObject>> = unsafe {
            msg_send![class!(NSEvent), addGlobalMonitorForEventsMatchingMask: ((1usize << 1) | (1usize << 3)), handler: &*handler]
        };
        Ok(Self {
            monitor: monitor.context("macOS 外部鼠标监听注册失败")?,
        })
    }
}

impl Drop for OutsideClickMonitor {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![class!(NSEvent), removeMonitor: &*self.monitor];
        }
    }
}
