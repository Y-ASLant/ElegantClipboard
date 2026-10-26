use anyhow::{Result, anyhow};
use clipboard_rs::ClipboardContext;

#[cfg(target_os = "linux")]
#[path = "unix/x11_sequence.rs"]
mod x11_sequence;

pub(crate) fn new_clipboard() -> Result<ClipboardContext> {
    let clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("初始化剪贴板失败：{error}"))?;
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && !matches!(&clipboard, ClipboardContext::Wayland(_))
    {
        anyhow::bail!(
            "Wayland 合成器未提供剪贴板 data-control 协议，无法监听系统剪贴板；请使用支持该协议的桌面或 X11 会话"
        );
    }
    #[cfg(target_os = "linux")]
    if matches!(&clipboard, ClipboardContext::X11(_)) {
        x11_sequence::current()?;
    }
    Ok(clipboard)
}

/// The identity returned in Copied/Merged events; zero is reserved for an unavailable clipboard.
pub fn clipboard_sequence() -> Result<u32> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        native_sequence()
    }
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return x11_sequence::current();
        }
        current_sequence(&new_clipboard()?)
    }
}

pub(crate) fn current_sequence(clipboard: &ClipboardContext) -> Result<u32> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        let _ = clipboard;
        native_sequence()
    }
    #[cfg(target_os = "linux")]
    {
        match clipboard {
            ClipboardContext::X11(_) => x11_sequence::current(),
            ClipboardContext::Wayland(_) => linux_content_identity(clipboard),
        }
    }
}

#[cfg(windows)]
fn native_sequence() -> Result<u32> {
    Ok(unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() })
}

#[cfg(target_os = "macos")]
fn native_sequence() -> Result<u32> {
    crate::macos::desktop::clipboard_sequence()
}

// The Wayland data-control wrapper exposes payloads but not selection generations.
// Compare every offered payload rather than its text-only upstream watcher.
#[cfg(target_os = "linux")]
fn linux_content_identity(clipboard: &ClipboardContext) -> Result<u32> {
    use clipboard_rs::Clipboard;
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut formats = clipboard
        .available_formats()
        .map_err(|error| anyhow!("读取剪贴板格式失败：{error}"))?;
    formats.sort_unstable();
    let mut hash = DefaultHasher::new();
    if formats.is_empty() {
        return Ok(1);
    }
    let mut readable = false;
    for format in formats {
        if matches!(
            format.as_str(),
            "TARGETS" | "TIMESTAMP" | "MULTIPLE" | "SAVE_TARGETS" | "DELETE" | "INCR"
        ) {
            continue;
        }
        format.hash(&mut hash);
        if let Ok(bytes) = clipboard.get_buffer(&format) {
            bytes.hash(&mut hash);
            readable = true;
        }
    }
    if !readable {
        anyhow::bail!("剪贴板没有可读取的数据，无法确认内容身份");
    }
    let hash = hash.finish();
    Ok(((hash ^ (hash >> 32)) as u32).max(1))
}
