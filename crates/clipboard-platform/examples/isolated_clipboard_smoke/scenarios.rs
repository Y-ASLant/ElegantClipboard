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
    events: &async_channel::Receiver<Event>,
    preview: &str,
) -> anyhow::Result<i64> {
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
    let native_id = wait_snapshot_by_preview(events, &native_text)?;
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
    events: &async_channel::Receiver<Event>,
    clipboard: &ClipboardContext,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let prefix = format!("transient-{}-", std::process::id());
    clipboard
        .set_text(format!("{prefix}first"))
        .map_err(|error| anyhow!("写入瞬时首个样本失败：{error}"))?;
    thread::sleep(Duration::from_millis(150));
    let final_text = format!("{prefix}final");
    clipboard
        .set_text(final_text.clone())
        .map_err(|error| anyhow!("写入瞬时最终样本失败：{error}"))?;
    wait_snapshot_by_preview(events, &final_text)?;
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

/// 决策 0003 的四个变体：去重策略切换、文本转换复制、按格式复制、条目备注。
pub(super) fn core_paste_variants(
    service: &Service,
    events: &async_channel::Receiver<Event>,
    clipboard: &ClipboardContext,
    data_dir: &std::path::Path,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use clipboard_core::ContentCategory;
    use clipboard_core::TextTransform;
    use clipboard_core::database::ClipboardItem as DbSnapshotItem;
    use clipboard_core::preferences::DedupStrategy;
    use clipboard_platform::CopyFormat;

    let history = || clipboard_core::History::open(data_dir.join("clipboard.db"));
    let wait_history = |search: &str| -> anyhow::Result<Vec<DbSnapshotItem>> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let rows = history()?.list(search, 100, false)?;
            if !rows.is_empty() {
                return Ok(rows);
            }
            if Instant::now() >= deadline {
                bail!("等待历史记录超时：{search}");
            }
            thread::sleep(Duration::from_millis(20));
        }
    };
    let status = || -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match events.try_recv() {
                Ok(Event::Copied { .. })
                | Ok(Event::Status(_))
                | Ok(Event::NoteSaved(_))
                | Ok(Event::DedupStrategySaved(_))
                | Ok(Event::RestoreClipboardSaved(_))
                | Ok(Event::ClipboardRestored(_)) => return Ok(()),
                Ok(Event::CommandFailed { message, .. }) => bail!("命令失败：{message}"),
                Ok(Event::Error(message)) => bail!("{message}"),
                Ok(_) => {}
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                bail!("等待命令确认超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };
    let wait_for_snapshot =
        |predicate: &dyn Fn(&[DbSnapshotItem]) -> bool| -> anyhow::Result<bool> {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                match events.try_recv() {
                    Ok(Event::Snapshot { items, .. }) if predicate(&items) => return Ok(true),
                    Ok(Event::BackgroundError(message)) | Ok(Event::Error(message)) => {
                        bail!("{message}")
                    }
                    Ok(_) => {}
                    Err(_) => {}
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                thread::sleep(Duration::from_millis(10));
            }
        };

    // 备注持久化并参与搜索。
    service.send(Command::Capture("note smoke".into()))?;
    let note_id = wait_history("note smoke")?
        .first()
        .context("缺少备注冒烟记录")?
        .id;
    service.send(Command::SetNote {
        id: note_id,
        note: "smoke 备注".into(),
    })?;
    status()?;
    service.send(Command::Query {
        search: "smoke 备注".into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: 100,
        generation: 0,
    })?;
    assert!(
        wait_for_snapshot(&|items| items
            .iter()
            .any(|item| item.id == note_id && item.note.as_deref() == Some("smoke 备注")))?,
        "备注未参与搜索或未随快照返回"
    );

    // 文本转换复制。
    service.send(Command::CopyTransformed {
        id: note_id,
        transform: TextTransform::Uppercase,
    })?;
    status()?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回转换文本失败：{error}"))?,
        "NOTE SMOKE"
    );

    // 按格式复制：先捕一条富文本，再分别取 HTML 与 RTF。
    service.send(Command::CaptureRich {
        html: Some("<b>format smoke</b>".into()),
        rtf: Some(b"{\\rtf1 format smoke}".as_slice().to_vec()),
        text: Some("format smoke".into()),
    })?;
    let rich_id = wait_history("format smoke")?
        .first()
        .context("缺少按格式冒烟记录")?
        .id;
    service.send(Command::CopyAsFormat {
        id: rich_id,
        format: CopyFormat::Html,
    })?;
    status()?;
    assert!(
        clipboard
            .get_html()
            .map_err(|error| anyhow!("读回 HTML 失败：{error}"))?
            .contains("format smoke")
    );
    service.send(Command::CopyAsFormat {
        id: rich_id,
        format: CopyFormat::Rtf,
    })?;
    status()?;
    let rtf_actual = clipboard
        .get_buffer("Rich Text Format")
        .map_err(|error| anyhow!("读回 RTF 失败：{error}"))?;
    // 写回的 RTF 带系统惯例的 NUL 终止符。
    assert_eq!(
        rtf_actual
            .strip_suffix([0u8].as_slice())
            .unwrap_or(&rtf_actual),
        b"{\\rtf1 format smoke}".as_slice()
    );

    // 去重策略：always_new 下同文本产生新条目；切回后合并。
    service.send(Command::SetDedupStrategy(DedupStrategy::AlwaysNew))?;
    status()?;
    service.send(Command::Capture("dedup smoke".into()))?;
    wait_history("dedup smoke")?;
    service.send(Command::Capture("dedup smoke".into()))?;
    let mut dedup_count = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        dedup_count = history()?.list("dedup smoke", 100, false)?.len();
        if dedup_count >= 2 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(dedup_count, 2, "always_new 应各存一条");
    service.send(Command::SetDedupStrategy(DedupStrategy::MoveToTop))?;
    status()?;
    service.send(Command::Capture("dedup smoke".into()))?;
    thread::sleep(Duration::from_millis(200));
    let merged_count = history()?.list("dedup smoke", 100, false)?.len();
    assert_eq!(merged_count, 2, "move_to_top 不应新增第三条");
    Ok(())
}
