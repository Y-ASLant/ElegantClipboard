#[cfg(not(windows))]
fn main() {
    eprintln!("此测试仅支持 Windows");
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, anyhow, bail};
    use clipboard_core::PreviewContent;
    use clipboard_platform::{Command, Event, Service};
    use clipboard_rs::{
        Clipboard, ClipboardContent, ClipboardContext, ContentFormat, RustImageData,
        common::RustImage,
    };
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::Foundation::HANDLE,
        Win32::System::DataExchange::GetClipboardSequenceNumber,
        Win32::System::StationsAndDesktops::{
            CreateDesktopW, CreateWindowStationW, DESKTOP_CONTROL_FLAGS, GetUserObjectInformationW,
            SetProcessWindowStation, SetThreadDesktop, UOI_NAME,
        },
        core::{PCWSTR, w},
    };

    // This executable creates its own window station before using USER32.
    // The interactive WinSta0 clipboard is never selected for writes.
    let station = unsafe { CreateWindowStationW(PCWSTR::null(), 0, 0x37f, None) }?;
    let mut station_name = [0u16; 256];
    unsafe {
        GetUserObjectInformationW(
            HANDLE(station.0),
            UOI_NAME,
            Some(station_name.as_mut_ptr().cast()),
            std::mem::size_of_val(&station_name) as u32,
            None,
        )
    }?;
    let name_end = station_name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(station_name.len());
    let station_name = String::from_utf16(&station_name[..name_end])?;
    if station_name.eq_ignore_ascii_case("WinSta0") {
        bail!("当前窗口站是交互窗口站，拒绝测试");
    }
    unsafe { SetProcessWindowStation(station) }?;
    let desktop = unsafe {
        CreateDesktopW(
            w!("ClipboardQA"),
            PCWSTR::null(),
            None,
            DESKTOP_CONTROL_FLAGS(0),
            0x1ff,
            None,
        )
    }?;
    unsafe { SetThreadDesktop(desktop) }?;

    let clipboard =
        ClipboardContext::new().map_err(|error| anyhow!("初始化隔离剪贴板失败：{error}"))?;
    clipboard
        .clear()
        .map_err(|error| anyhow!("重置隔离剪贴板失败：{error}"))?;
    println!("window station: {station_name}");
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), true)?;
    thread::sleep(Duration::from_millis(500));

    let snapshot = |kind: &str| -> anyhow::Result<i64> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match events.try_recv() {
                Ok(Event::Snapshot { items, .. }) => {
                    if let Some(item) = items.iter().find(|item| item.content_type == kind) {
                        return Ok(item.id);
                    }
                }
                Ok(Event::Error(message)) => bail!("{message}"),
                _ => {}
            }
            if Instant::now() >= deadline {
                bail!("等待 {kind} 采集超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };
    let status = |expected_paste: bool| -> anyhow::Result<u32> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match events.try_recv() {
                Ok(Event::Copied {
                    request_id,
                    clipboard_sequence,
                    ..
                }) => {
                    assert_eq!(request_id.is_some(), expected_paste);
                    return Ok(clipboard_sequence);
                }
                Ok(Event::CommandFailed { message, .. }) => bail!("复制命令失败：{message}"),
                Ok(Event::Error(message)) => bail!("{message}"),
                _ => {}
            }
            if Instant::now() >= deadline {
                bail!("等待复制确认超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };
    let merged_status = |expected_count: usize| -> anyhow::Result<u32> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match events.try_recv() {
                Ok(Event::Merged {
                    for_paste,
                    clipboard_sequence,
                    item_count,
                }) => {
                    assert!(!for_paste);
                    assert_eq!(item_count, expected_count);
                    return Ok(clipboard_sequence);
                }
                Ok(Event::CommandFailed { message, .. }) => {
                    bail!("合并复制命令失败：{message}")
                }
                Ok(Event::Error(message)) => bail!("{message}"),
                _ => {}
            }
            if Instant::now() >= deadline {
                bail!("等待合并复制确认超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };

    // Privacy probes write only to the private window station created above.
    let private_text = format!("private-history-{}", std::process::id());
    for (format, payload) in [
        ("ExcludeClipboardContentFromMonitorProcessing", vec![1]),
        ("CanIncludeInClipboardHistory", 0u32.to_le_bytes().to_vec()),
    ] {
        clipboard
            .set(vec![
                ClipboardContent::Text(private_text.clone()),
                ClipboardContent::Other(format.into(), payload),
            ])
            .map_err(|error| anyhow!("写入隔离隐私样本失败：{error}"))?;
        // A following query is also a worker barrier; the watcher needs one interval.
        thread::sleep(Duration::from_millis(200));
        service.send(Command::Query {
            search: private_text.clone(),
            favorite_only: false,
            category: clipboard_core::ContentCategory::All,
            group_id: None,
            limit: clipboard_core::PAGE_SIZE,
            generation: 90,
        })?;
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Ok(Event::Snapshot {
                items,
                generation: 90,
                ..
            }) = events.try_recv()
            {
                assert!(
                    items.is_empty(),
                    "private clipboard content entered history"
                );
                break;
            }
            if Instant::now() >= deadline {
                bail!("等待隐私标记验证超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: clipboard_core::ContentCategory::All,
        group_id: None,
        limit: clipboard_core::PAGE_SIZE,
        generation: 91,
    })?;
    println!("clipboard privacy exclusions ok");

    let text = format!("isolated-text-{}", std::process::id());
    clipboard
        .set_text(text.clone())
        .map_err(|error| anyhow!("写入测试文本失败：{error}"))?;
    let text_id = snapshot("text")?;
    service.send(Command::CopyForPaste(text_id, 1))?;
    let copied_sequence = status(true)?;
    assert_ne!(copied_sequence, 0);
    assert_eq!(unsafe { GetClipboardSequenceNumber() }, copied_sequence);
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回文本失败：{error}"))?,
        text
    );
    clipboard
        .set_text("clipboard changed before paste".into())
        .map_err(|error| anyhow!("模拟外部改写剪贴板失败：{error}"))?;
    assert_ne!(unsafe { GetClipboardSequenceNumber() }, copied_sequence);
    println!("text capture/copy ok");

    let html = "<b>rich smoke</b>";
    let rtf = b"{\\rtf1\\bin2 \x00\xff rich smoke}\0";
    service.send(Command::CaptureRich {
        html: Some(html.into()),
        rtf: Some(rtf.to_vec()),
        text: Some("rich smoke".into()),
    })?;
    let rich_id = snapshot("html")?;
    service.send(Command::Preview {
        id: rich_id,
        generation: 1,
    })?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::Preview { result, .. }) => {
                assert_eq!(
                    result.map_err(|error| anyhow!(error))?,
                    PreviewContent::Html("<b>rich smoke</b>".into())
                );
                break;
            }
            Ok(Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待富文本预览超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
    service.send(Command::Copy(rich_id))?;
    status(false)?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回富文本纯文本失败：{error}"))?,
        "rich smoke"
    );
    assert!(
        clipboard
            .get_html()
            .map_err(|error| anyhow!("读回 HTML 失败：{error}"))?
            .contains(html)
    );
    println!(
        "rich formats: {:?}",
        clipboard
            .available_formats()
            .map_err(|error| anyhow!("列出富文本格式失败：{error}"))?
    );
    assert_eq!(
        clipboard
            .get_buffer("Rich Text Format")
            .map_err(|error| anyhow!("读回 RTF 失败：{error}"))?,
        rtf
    );
    println!("rich capture/copy ok");

    service.send(Command::CopyPlainText(rich_id))?;
    status(false)?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回纯文本失败：{error}"))?,
        "rich smoke"
    );
    assert!(!clipboard.has(ContentFormat::Html));
    assert!(!clipboard.has(ContentFormat::Rtf));
    println!("rich plain-text copy ok");

    let observed_html = "<b>observed rich snapshot</b>";
    let observed_rtf = b"{\\rtf1 observed rich snapshot}\0";
    clipboard
        .set(vec![
            ClipboardContent::Html(observed_html.into()),
            ClipboardContent::Other("Rich Text Format".into(), observed_rtf.to_vec()),
            ClipboardContent::Text("observed rich snapshot".into()),
        ])
        .map_err(|error| anyhow!("写入监听富文本样本失败：{error}"))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::Snapshot { items, .. })
                if items.iter().any(|item| {
                    item.content_type == "html"
                        && item.preview.as_deref() == Some("observed rich snapshot")
                }) =>
            {
                break;
            }
            Ok(Event::BackgroundError(message) | Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待多格式监听采集超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let history = clipboard_core::History::open(directory.path().join("clipboard.db"))?;
    let rows = history.list("observed rich snapshot", 100, false)?;
    let observed = history.item(rows.first().context("缺少监听富文本记录")?.id)?;
    let clipboard_html = clipboard.get_html().map_err(|error| anyhow!("{error}"))?;
    assert!(clipboard_html.contains(observed_html));
    assert_eq!(
        observed.html_content.as_deref(),
        Some(clipboard_html.as_str())
    );
    assert_eq!(
        observed.text_content.as_deref(),
        Some("observed rich snapshot")
    );
    assert_eq!(
        clipboard_core::rich::decode_rtf_for_clipboard(observed.rtf_content.as_deref().unwrap()),
        Some(observed_rtf.to_vec())
    );
    drop(history);
    clipboard
        .set(vec![ClipboardContent::Other(
            "Rich Text Format".into(),
            b"\\*\\datastore\0".to_vec(),
        )])
        .map_err(|error| anyhow!("写入异常 RTF 样本失败：{error}"))?;
    let malformed_id = snapshot("rtf")?;
    service.send(Command::Copy(malformed_id))?;
    status(false)?;
    assert_eq!(
        clipboard
            .get_buffer("Rich Text Format")
            .map_err(|error| anyhow!("{error}"))?,
        b"\\*\\datastore\0"
    );
    println!("observed rich snapshot and malformed RTF roundtrip ok");

    let image_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.png");
    let image = RustImageData::from_path(image_path.to_str().context("测试图片路径不是 UTF-8")?)
        .map_err(|error| anyhow!("读取测试图片失败：{error}"))?;
    clipboard
        .set_image(image)
        .map_err(|error| anyhow!("写入测试图片失败：{error}"))?;
    let image_id = snapshot("image")?;
    service.send(Command::Copy(image_id))?;
    status(false)?;
    let image = clipboard
        .get_image()
        .map_err(|error| anyhow!("读回图片失败：{error}"))?;
    assert_eq!(image.get_size(), (128, 128));
    println!("image capture/copy ok");

    let file = directory.path().join("file.txt");
    std::fs::write(&file, "isolated file")?;
    let file = file.to_str().context("测试文件路径不是 UTF-8")?.to_owned();
    clipboard
        .set_files(vec![file.clone()])
        .map_err(|error| anyhow!("写入测试文件失败：{error}"))?;
    let file_id = snapshot("files")?;
    service.send(Command::Copy(file_id))?;
    status(false)?;
    assert_eq!(
        clipboard
            .get_files()
            .map_err(|error| anyhow!("读回文件失败：{error}"))?,
        vec![file.clone()]
    );
    println!("files capture/copy ok");

    service.send(Command::CopyPath(file_id))?;
    status(false)?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回文件路径文本失败：{error}"))?,
        file
    );
    assert!(!clipboard.has(ContentFormat::Files));
    println!("file path copy ok");

    service.send(Command::MergeCopy(vec![text_id, rich_id, file_id]))?;
    let merged_sequence = merged_status(3)?;
    assert_ne!(merged_sequence, 0);
    assert_eq!(unsafe { GetClipboardSequenceNumber() }, merged_sequence);
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回合并文本失败：{error}"))?,
        format!("{text}\nrich smoke\n{file}")
    );
    assert_eq!(
        clipboard
            .get_files()
            .map_err(|error| anyhow!("读回合并文件失败：{error}"))?,
        vec![file]
    );
    println!("merged text/files copy ok");

    clipboard
        .set_text("x".repeat(clipboard_core::MAX_TEXT_BYTES + 1))
        .map_err(|error| anyhow!("写入超限测试文本失败：{error}"))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::BackgroundError(message)) => {
                assert!(message.contains("文本超过 1 MiB"), "{message}");
                break;
            }
            Ok(Event::Error(message)) => bail!("超限采集被报告为操作错误：{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待超限采集错误超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
    println!("capture error isolation ok");

    native_format_roundtrip(&service, &events, &clipboard, directory.path())?;
    println!("native format fidelity roundtrip ok");
    transient_filtering(&events, &clipboard, directory.path())?;
    println!("transient window filtering ok");
    core_paste_variants(&service, &events, &clipboard, directory.path())?;
    println!("dedup/transform/format/note variants ok");

    drop(service);
    println!("isolated clipboard roundtrip passed");
    Ok(())
}

/// 原生格式保真与瞬时过滤场景（拆分自本文件，保持 500 行限制）。
#[cfg(windows)]
mod scenarios;

#[cfg(windows)]
use scenarios::{core_paste_variants, native_format_roundtrip, transient_filtering};
