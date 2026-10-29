use anyhow::{Result, anyhow};
use notify_rust::{Hint, Notification, Timeout};
use std::sync::Mutex;

static REPLACES_ID: Mutex<u32> = Mutex::new(0);

pub(super) fn show(title: &str, message: &str) -> Result<()> {
    let mut id = REPLACES_ID
        .lock()
        .map_err(|_| anyhow!("系统通知锁不可用"))?;
    let notification = Notification::new()
        .appname("ElegantClipboard")
        .summary(title)
        .body(message)
        .icon("edit-paste")
        .hint(Hint::SuppressSound(true))
        .timeout(Timeout::Default)
        .id(*id)
        .show()?;
    *id = notification.id();
    Ok(())
}
