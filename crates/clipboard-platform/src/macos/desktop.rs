use super::cocoa;
use anyhow::{Context, Result, bail};
use clipboard_core::preferences::PasteKeyPreference;
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CFRelease(object: *const c_void);
}

pub fn foreground_target() -> Option<(isize, u32)> {
    objc2::rc::autoreleasepool(|_| {
        let app: Option<Retained<AnyObject>> =
            unsafe { msg_send![&cocoa::workspace(), frontmostApplication] };
        let pid: i32 = unsafe { msg_send![&app?, processIdentifier] };
        (pid > 0).then_some((pid as isize, pid as u32))
    })
}

fn application(target: (isize, u32)) -> Option<Retained<AnyObject>> {
    if target.0 != target.1 as isize || target.1 == 0 || target.1 > i32::MAX as u32 {
        return None;
    }
    unsafe {
        msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: target.1 as i32]
    }
}

pub fn target_alive(target: (isize, u32)) -> bool {
    application(target).is_some_and(|app| {
        let terminated: bool = unsafe { msg_send![&app, isTerminated] };
        !terminated
    })
}

pub fn require_accessibility() -> Result<()> {
    if !unsafe { AXIsProcessTrusted() } {
        bail!(
            "自动粘贴需要 macOS 系统设置 → 隐私与安全性 → 辅助功能权限；请允许 ElegantClipboard 后重试"
        );
    }
    Ok(())
}

pub fn unavailable_reason() -> Option<&'static str> {
    (!unsafe { AXIsProcessTrusted() })
        .then_some("自动粘贴需要 macOS 系统设置 → 隐私与安全性 → 辅助功能权限")
}

pub fn activate_target(target: (isize, u32)) -> Result<()> {
    require_accessibility()?;
    let app = application(target).context("目标应用已退出，内容已复制，请手动粘贴")?;
    let activated: bool = unsafe { msg_send![&app, activateWithOptions: 2usize] };
    if !activated {
        bail!("macOS 未允许激活目标应用，请手动粘贴")
    }
    Ok(())
}

pub fn modifiers_pressed() -> bool {
    unsafe { CGEventSourceFlagsState(1) & ((1 << 17) | (1 << 18) | (1 << 19) | (1 << 20)) != 0 }
}

pub fn shift_pressed() -> bool {
    unsafe { CGEventSourceFlagsState(1) & (1 << 17) != 0 }
}

pub fn send_paste(preference: PasteKeyPreference) -> Result<()> {
    require_accessibility()?;
    if preference == PasteKeyPreference::ShiftInsert {
        bail!("macOS 不支持 Shift+Insert 自动粘贴，请选择 Command+V");
    }
    if modifiers_pressed() {
        bail!("请松开修饰键后再粘贴")
    }
    // Explicit flags on V synthesize Command+V without releasing user-owned keys.
    let down = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), 9, true) };
    let up = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), 9, false) };
    if down.is_null() || up.is_null() {
        unsafe {
            if !down.is_null() {
                CFRelease(down)
            }
            if !up.is_null() {
                CFRelease(up)
            }
        }
        bail!("无法创建 macOS 粘贴键盘事件");
    }
    unsafe {
        CGEventSetFlags(down, 1 << 20);
        CGEventSetFlags(up, 1 << 20);
        CGEventPost(0, down);
        CGEventPost(0, up);
        CFRelease(down);
        CFRelease(up);
    }
    Ok(())
}

pub fn clipboard_sequence() -> Result<u32> {
    objc2::rc::autoreleasepool(|_| {
        let pasteboard: Option<Retained<AnyObject>> =
            unsafe { msg_send![class!(NSPasteboard), generalPasteboard] };
        let pasteboard = pasteboard.context("无法访问 macOS 系统剪贴板")?;
        let count: isize = unsafe { msg_send![&pasteboard, changeCount] };
        Ok(count as u32)
    })
}
