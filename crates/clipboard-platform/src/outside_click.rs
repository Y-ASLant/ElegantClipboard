use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use std::{
    cell::RefCell,
    sync::mpsc,
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, GetWindowThreadProcessId, HC_ACTION, IsWindowVisible, MSG,
        MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WindowFromPoint,
    },
};

thread_local! {
    static CLICK_EVENTS: RefCell<Option<(Sender<POINT>, isize)>> = const { RefCell::new(None) };
}

unsafe extern "system" fn mouse_hook(code: i32, button: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && matches!(button.0 as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
        // A low-level hook runs on the installing thread. Never wait for the UI here.
        let position = unsafe { (*(data.0 as *const MSLLHOOKSTRUCT)).pt };
        CLICK_EVENTS.with(|events| {
            if let Some((sender, hwnd)) = events.borrow().as_ref()
                && unsafe { IsWindowVisible(HWND(*hwnd as *mut _)) }.as_bool()
                && !is_current_process_window(position)
            {
                let _ = sender.try_send(position);
            }
        });
    }
    unsafe { CallNextHookEx(None, code, button, data) }
}

fn is_current_process_window(position: POINT) -> bool {
    let clicked_window = unsafe { WindowFromPoint(position) };
    let mut process_id = 0;
    unsafe { GetWindowThreadProcessId(clicked_window, Some(&mut process_id)) };
    process_id == unsafe { GetCurrentProcessId() }
}

pub struct OutsideClickMonitor {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl OutsideClickMonitor {
    pub fn start(events: Sender<POINT>, window_handle: isize) -> Result<Self> {
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("outside-click-monitor".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                CLICK_EVENTS.with(|slot| *slot.borrow_mut() = Some((events, window_handle)));
                let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0) };
                let Ok(hook) = hook else {
                    CLICK_EVENTS.with(|slot| *slot.borrow_mut() = None);
                    let _ = ready.send(hook.map(|_| thread_id));
                    return;
                };
                let _ = ready.send(Ok(thread_id));
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {}
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                CLICK_EVENTS.with(|slot| *slot.borrow_mut() = None);
            })
            .context("无法启动外部点击监听线程")?;
        match receiver.recv().context("外部点击监听线程未响应")? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(anyhow!("外部点击监听注册失败：{error}"))
            }
        }
    }
}

impl Drop for OutsideClickMonitor {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
