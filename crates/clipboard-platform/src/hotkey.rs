use anyhow::{Context, Result, anyhow};
use async_channel::Sender;
use clipboard_core::preferences::HotkeyPreference;
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
        },
        WindowsAndMessaging::{
            GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
        },
    },
};

const HOTKEY_ID: i32 = 1;

pub struct Hotkey {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl Hotkey {
    pub fn start(choice: HotkeyPreference, events: Sender<()>) -> Result<Self> {
        let (modifiers, key) = match choice {
            HotkeyPreference::CtrlShiftV => (MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, b'V'),
            HotkeyPreference::AltC => (MOD_ALT | MOD_NOREPEAT, b'C'),
            HotkeyPreference::CtrlAltV => (MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, b'V'),
            HotkeyPreference::Disabled => return Err(anyhow!("快捷键已关闭")),
        };
        let (ready, receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("history-hotkey".into())
            .spawn(move || {
                let mut message = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                }
                let thread_id = unsafe { GetCurrentThreadId() };
                let registration =
                    unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers, key as u32) };
                let registered = registration.is_ok();
                let _ = ready.send(registration.map(|_| thread_id));
                if !registered {
                    return;
                }
                while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
                    if message.message == WM_HOTKEY && message.wParam.0 == HOTKEY_ID as usize {
                        let _ = events.try_send(());
                    }
                }
                unsafe {
                    let _ = UnregisterHotKey(None, HOTKEY_ID);
                }
            })?;
        match receiver.recv().context("快捷键线程未响应")? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Err(error) => {
                let _ = worker.join();
                Err(anyhow!("{} 注册失败：{error}", choice.label()))
            }
        }
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
