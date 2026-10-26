use super::capture::WatchHandler;
use anyhow::{Result, anyhow};
use clipboard_rs::{ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

pub(super) fn start(
    handler: WatchHandler,
    stop: Arc<AtomicBool>,
) -> Result<(Option<WatcherShutdown>, JoinHandle<()>)> {
    #[cfg(target_os = "linux")]
    if matches!(
        &handler.clipboard,
        clipboard_rs::ClipboardContext::Wayland(_)
    ) {
        return start_wayland(handler, stop).map(|worker| (None, worker));
    }
    let mut watcher =
        ClipboardWatcherContext::new().map_err(|error| anyhow!("初始化监听失败：{error}"))?;
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && !matches!(&watcher, ClipboardWatcherContext::Wayland(_))
    {
        anyhow::bail!("Wayland 剪贴板监听不可用，拒绝退回仅能读取 X11 应用的监听器");
    }
    let events = handler.events.clone();
    let feedback = handler.feedback.clone();
    watcher.add_handler(handler);
    let shutdown = watcher.get_shutdown_channel();
    let worker = thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || {
            watcher.start_watch();
            if !stop.load(Ordering::Acquire) {
                feedback.error("剪贴板监听已停止，请重启应用".into());
                feedback.flush(&events);
            }
        })?;
    Ok((Some(shutdown), worker))
}

#[cfg(target_os = "linux")]
fn start_wayland(mut handler: WatchHandler, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let mut last_error = None;
    // clipboard-rs' Wayland watcher compares only MIME names and plain text, which
    // misses repeated image/file/RTF copies. CaptureState compares every offered payload.
    Ok(thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match handler.read_change() {
                    Ok(()) => last_error = None,
                    Err(error) => {
                        let error = error.to_string();
                        if last_error.as_ref() != Some(&error) {
                            handler.feedback.error(error.clone());
                            last_error = Some(error);
                        }
                    }
                }
                handler.feedback.flush(&handler.events);
                thread::sleep(std::time::Duration::from_millis(250));
            }
        })?)
}
