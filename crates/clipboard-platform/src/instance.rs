use crate::windows::Event;
use anyhow::{Context, Result};
use async_channel::Sender;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
        },
    },
    core::PCWSTR,
};

/// 以 NUL 结尾的 UTF-16 编码，供 Win32 宽字符 API 直接使用。
pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// [`to_wide`] 结果的小端字节流，供按字节传递的 API（注册表、剪贴板）使用。
pub(crate) fn to_wide_bytes(s: &str) -> Vec<u8> {
    to_wide(s)
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

fn event_name(data_dir: &Path) -> Result<Vec<u16>> {
    let path = std::fs::canonicalize(data_dir).context("无法确定数据目录绝对路径")?;
    let hash = clipboard_core::backup::hash_token(path.to_string_lossy().to_lowercase().as_bytes());
    Ok(to_wide(&format!("Local\\ElegantClipboard-GPUI-{hash}")))
}

pub fn show_existing(data_dir: &Path) -> Result<bool> {
    if !data_dir.is_dir() {
        return Ok(false);
    }
    let name = event_name(data_dir)?;
    let Ok(handle) = (unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) })
    else {
        return Ok(false);
    };
    let result = unsafe { SetEvent(handle) }.context("无法唤出正在运行的窗口");
    unsafe { CloseHandle(handle) }?;
    result?;
    Ok(true)
}

pub struct InstanceSignal {
    handle: HANDLE,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    #[cfg(test)]
    pub(crate) wake_received: std::sync::mpsc::Receiver<()>,
}

impl InstanceSignal {
    pub fn start(data_dir: &Path, events: Sender<Event>) -> Result<Self> {
        let name = event_name(data_dir)?;
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())) }
            .context("无法创建实例唤出事件")?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let raw_handle = handle.0 as usize;
        #[cfg(test)]
        let (wake_started, wake_received) = std::sync::mpsc::channel();
        let worker = match thread::Builder::new()
            .name("instance-signal".into())
            .spawn(move || {
                let handle = HANDLE(raw_handle as *mut _);
                while unsafe { WaitForSingleObject(handle, INFINITE) } == WAIT_OBJECT_0 {
                    if worker_stop.load(Ordering::Acquire) {
                        break;
                    }
                    #[cfg(test)]
                    let _ = wake_started.send(());
                    if events.send_blocking(Event::ShowWindow).is_err() {
                        break;
                    }
                }
            }) {
            Ok(worker) => worker,
            Err(error) => {
                unsafe { CloseHandle(handle) }?;
                return Err(error.into());
            }
        };
        Ok(Self {
            handle,
            stop,
            worker: Some(worker),
            #[cfg(test)]
            wake_received,
        })
    }
}

impl Drop for InstanceSignal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        unsafe {
            let _ = SetEvent(self.handle);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
