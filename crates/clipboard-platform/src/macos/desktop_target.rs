//! 粘贴目标持有真实 AX 窗口和应用实例，不将其伪装成 CGWindowID。
use super::super::cocoa;
use anyhow::{Context, Result, bail};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::{cell::RefCell, ffi::c_void, ptr::NonNull};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *const c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *const c_void,
    ) -> i32;
    fn AXUIElementGetPid(element: *const c_void, pid: *mut i32) -> i32;
    fn AXUIElementGetTypeID() -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFGetTypeID(value: *const c_void) -> usize;
    fn CFEqual(left: *const c_void, right: *const c_void) -> u8;
    fn CFRelease(value: *const c_void);
}

struct OwnedCf(NonNull<c_void>);

impl OwnedCf {
    // 仅接收 Create/Copy 返回的所有权；对外元组不携带可解引用的指针。
    unsafe fn from_owned(value: *const c_void) -> Option<Self> {
        NonNull::new(value.cast_mut()).map(Self)
    }

    fn pointer(&self) -> *const c_void {
        self.0.as_ptr()
    }

    fn attribute(&self, name: &str) -> Option<Self> {
        let name = NSString::from_str(name);
        let mut value = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(
                self.pointer(),
                (&*name as *const NSString).cast(),
                &mut value,
            )
        };
        let owned = unsafe { Self::from_owned(value) };
        if error == 0 { owned } else { None }
    }

    fn focused_window(&self, pid: u32) -> Option<Self> {
        let window = self.attribute("AXFocusedWindow")?;
        if unsafe { CFGetTypeID(window.pointer()) } != unsafe { AXUIElementGetTypeID() } {
            return None;
        }
        let mut owner = 0;
        if unsafe { AXUIElementGetPid(window.pointer(), &mut owner) } != 0 || owner != pid as i32 {
            return None;
        }
        // 持有 AX 代理不代表远端窗口仍存在；查询属性确认对象有效后再比较身份。
        let role = window.attribute("AXRole")?;
        let window_role = NSString::from_str("AXWindow");
        (unsafe { CFEqual(role.pointer(), (&*window_role as *const NSString).cast()) } != 0)
            .then_some(window)
    }

    fn same_element(&self, other: &Self) -> bool {
        unsafe { CFEqual(self.pointer(), other.pointer()) != 0 }
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.pointer()) };
    }
}

struct CapturedTarget {
    application: Retained<AnyObject>,
    accessibility: OwnedCf,
    window: OwnedCf,
    pid: u32,
}

impl CapturedTarget {
    fn capture(application: Retained<AnyObject>, pid: u32) -> Option<Self> {
        let accessibility =
            unsafe { OwnedCf::from_owned(AXUIElementCreateApplication(pid as i32)) }?;
        let window = accessibility.focused_window(pid)?;
        Some(Self {
            application,
            accessibility,
            window,
            pid,
        })
    }

    fn same_application(&self, application: &AnyObject) -> bool {
        let pid: i32 = unsafe { msg_send![application, processIdentifier] };
        // AppKit 明确要求通过 isEqual: 判断应用实例；持有原对象可安全识别 PID 复用。
        let same: bool = unsafe { msg_send![&self.application, isEqual: application] };
        same && pid == self.pid as i32
    }

    fn valid(&self) -> bool {
        let terminated: bool = unsafe { msg_send![&self.application, isTerminated] };
        if terminated {
            return false;
        }
        let current: Option<Retained<AnyObject>> = unsafe {
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: self.pid as i32]
        };
        current.is_some_and(|current| self.same_application(&current))
            && self.window.attribute("AXRole").is_some()
            && self
                .accessibility
                .focused_window(self.pid)
                .is_some_and(|focused| self.window.same_element(&focused))
    }
}

#[derive(Default)]
struct TargetSlot {
    token: isize,
    captured: Option<CapturedTarget>,
}

impl TargetSlot {
    fn next_token(&mut self) -> Option<isize> {
        // 即使令牌已耗尽，也必须释放旧窗口代理。
        self.captured = None;
        self.token = self.token.checked_add(1)?;
        Some(self.token)
    }
}

thread_local! {
    // Carbon 回调与 GPUI 粘贴都在主线程，最多保留一个外部目标。
    // 查询自己的前台窗口不能覆盖用户原来的粘贴目标。
    static TARGET: RefCell<TargetSlot> = RefCell::default();
}

pub fn foreground_target() -> Option<(isize, u32)> {
    cocoa::main_thread().ok()?;
    objc2::rc::autoreleasepool(|_| {
        let application: Option<Retained<AnyObject>> =
            unsafe { msg_send![&cocoa::workspace(), frontmostApplication] };
        let application = application?;
        let pid: i32 = unsafe { msg_send![&application, processIdentifier] };
        if pid <= 0 {
            return None;
        }
        let pid = pid as u32;
        if pid == std::process::id() {
            return Some((0, pid));
        }
        let captured = super::unavailable_reason()
            .is_none()
            .then(|| CapturedTarget::capture(application, pid))
            .flatten();
        TARGET.with(|target| {
            let mut target = target.try_borrow_mut().ok()?;
            let Some(captured) = captured else {
                target.captured = None;
                return Some((0, pid));
            };
            if target.captured.as_ref().is_some_and(|previous| {
                previous.same_application(&captured.application)
                    && previous.window.same_element(&captured.window)
            }) {
                return Some((target.token, pid));
            }
            // 令牌耗尽时停用自动粘贴，绝不循环复用旧令牌。
            target.next_token()?;
            target.captured = Some(captured);
            Some((target.token, pid))
        })
    })
}

pub fn target_alive(identity: (isize, u32)) -> bool {
    if identity.0 <= 0 || cocoa::main_thread().is_err() {
        return false;
    }
    objc2::rc::autoreleasepool(|_| {
        TARGET.with(|target| {
            let Ok(mut target) = target.try_borrow_mut() else {
                return false;
            };
            if target.token != identity.0 {
                return false;
            }
            let valid = super::unavailable_reason().is_none()
                && target
                    .captured
                    .as_ref()
                    .is_some_and(|captured| captured.pid == identity.1 && captured.valid());
            if !valid {
                // 检测到窗口或进程变化后，此令牌永久失效。
                target.captured = None;
            }
            valid
        })
    })
}

pub fn activate_target(identity: (isize, u32)) -> Result<()> {
    super::require_accessibility()?;
    cocoa::main_thread()?;
    if !target_alive(identity) {
        bail!("目标窗口已关闭，内容已复制，请手动粘贴");
    }
    objc2::rc::autoreleasepool(|_| {
        let application = TARGET
            .with(|target| {
                let target = target.try_borrow().ok()?;
                let captured = target.captured.as_ref()?;
                (target.token == identity.0
                    && identity.0 > 0
                    && captured.pid == identity.1
                    && captured.valid())
                .then(|| captured.application.clone())
            })
            .context("目标窗口已关闭，内容已复制，请手动粘贴")?;
        // 激活可能重入 AppKit 回调，先释放目标槽位借用。
        let activated: bool = unsafe { msg_send![&application, activateWithOptions: 0usize] };
        if !activated {
            bail!("macOS 未允许激活目标应用，请手动粘贴");
        }
        if !target_alive(identity) {
            bail!("前台窗口已变化，已取消自动粘贴，请检查后手动粘贴");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::TargetSlot;

    #[test]
    fn target_tokens_never_reuse_zero_or_a_previous_identity() {
        let mut slot = TargetSlot::default();
        let first = slot.next_token().unwrap();
        assert!(first > 0);
        // 清除所有权后仍保留代次；排队的旧事件不能匹配后来捕获的新目标。
        slot.captured = None;
        let second = slot.next_token().unwrap();
        assert!(second > first);
        slot.token = isize::MAX - 1;
        assert_eq!(slot.next_token(), Some(isize::MAX));
        assert_eq!(slot.next_token(), None);
        assert_eq!(slot.token, isize::MAX);
    }
}
