use anyhow::{Result, bail};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows::core::w;

/// Non-admin `CreateWindowStationW(NULL)` reuses the logon-session station.
/// Concurrent smoke processes therefore share one clipboard and can corrupt it.
pub(super) struct StationGuard(HANDLE);

impl StationGuard {
    pub(super) fn acquire() -> Result<Self> {
        let handle = unsafe {
            CreateMutexW(
                None,
                false,
                w!("Local\\ElegantClipboardIsolatedClipboardSmoke"),
            )
        }?;
        let wait = unsafe { WaitForSingleObject(handle, 120_000) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            let _ = unsafe { CloseHandle(handle) };
            bail!("另一个隔离剪贴板测试正占用同一登录窗口站，已拒绝并发运行");
        }
        Ok(Self(handle))
    }
}

impl Drop for StationGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
