//! 原生格式保真往返与瞬时内容过滤两个冒烟场景（由 isolated_clipboard_smoke 调用）。

use anyhow::{anyhow, bail};
use clipboard_platform::{Command, Event, Service};
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
use std::{
    thread,
    time::{Duration, Instant},
};

/// 等待一条 preview 文本匹配的 Snapshot 出现，返回其 id。
pub(super) fn wait_snapshot_by_preview(
    service: &Service,
    events: &async_channel::Receiver<Event>,
    preview: &str,
) -> anyhow::Result<i64> {
    let _ = service;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::Snapshot { items, .. }) => {
                if let Some(item) = items
                    .iter()
                    .find(|item| item.preview.as_deref() == Some(preview))
                {
                    return Ok(item.id);
                }
            }
            Ok(Event::BackgroundError(message) | Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待 {preview} 采集超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// 原生格式保真往返：带自定义注册格式的采集 → Copy 写回 → 枚举可见且字节一致。
pub(super) fn native_format_roundtrip(
    service: &Service,
    events: &async_channel::Receiver<Event>,
    clipboard: &ClipboardContext,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let native_payload: Vec<u8> = (0..512u32).map(|value| (value % 251) as u8).collect();
    let native_name = "ElegantClipboard QA Native Format";
    let native_text = format!("native-fidelity-{}", std::process::id());
    clipboard
        .set(vec![
            ClipboardContent::Text(native_text.clone()),
            ClipboardContent::Other(native_name.into(), native_payload.clone()),
        ])
        .map_err(|error| anyhow!("写入原生格式样本失败：{error}"))?;
    let native_id = wait_snapshot_by_preview(service, events, &native_text)?;
    {
        let history = clipboard_core::History::open(directory.join("clipboard.db"))?;
        let stored = history.native_formats_for_copy(native_id)?;
        assert!(
            stored
                .iter()
                .any(|(name, data)| name == native_name && data == &native_payload),
            "额外原生格式应随条目入库"
        );
    }
    service.send(Command::Copy(native_id))?;
    wait_copied(events)?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回原生样本文本失败：{error}"))?,
        native_text
    );
    let roundtripped = clipboard_format_list().and_then(|formats| {
        formats
            .contains(&native_name.to_owned())
            .then(|| clipboard.get_buffer(native_name).ok())
            .flatten()
    });
    assert_eq!(
        roundtripped.as_deref(),
        Some(native_payload.as_slice()),
        "自定义格式写回后应可按名称读回"
    );
    Ok(())
}

/// 瞬时内容：150ms 内覆盖写 → 仅最终内容入库。
pub(super) fn transient_filtering(
    service: &Service,
    events: &async_channel::Receiver<Event>,
    clipboard: &ClipboardContext,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let _ = service;
    let prefix = format!("transient-{}-", std::process::id());
    clipboard
        .set_text(format!("{prefix}first"))
        .map_err(|error| anyhow!("写入瞬时首个样本失败：{error}"))?;
    thread::sleep(Duration::from_millis(150));
    let final_text = format!("{prefix}final");
    clipboard
        .set_text(final_text.clone())
        .map_err(|error| anyhow!("写入瞬时最终样本失败：{error}"))?;
    wait_snapshot_by_preview(service, events, &final_text)?;
    // 给被丢弃条目的确认窗口留出时间，再核对历史。
    thread::sleep(Duration::from_millis(600));
    let history = clipboard_core::History::open(directory.join("clipboard.db"))?;
    assert!(
        history
            .list(&format!("{prefix}first"), 100, false)?
            .is_empty(),
        "被覆盖的瞬时内容不应入库"
    );
    assert_eq!(
        history.list(&final_text, 100, false)?.len(),
        1,
        "最终内容应入库"
    );
    Ok(())
}

fn wait_copied(events: &async_channel::Receiver<Event>) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::Copied { .. }) => return Ok(()),
            Ok(Event::CommandFailed { message, .. }) => bail!("复制命令失败：{message}"),
            Ok(Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待复制确认超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// 用 EnumClipboardFormats 列出当前剪贴板全部格式名（独立短会话）。
fn clipboard_format_list() -> Option<Vec<String>> {
    let clipboard = clipboard_win::Clipboard::new_attempts(10).ok()?;
    let mut names = Vec::new();
    for format in clipboard_win::EnumFormats::new() {
        let mut buffer = [0u16; 256];
        // SAFETY: buffer 是有效的输出缓冲；剪贴板已打开。
        let length = unsafe {
            windows::Win32::System::DataExchange::GetClipboardFormatNameW(format, &mut buffer)
        };
        if length > 0 {
            names.push(String::from_utf16_lossy(&buffer[..length as usize]));
        }
    }
    drop(clipboard);
    Some(names)
}
