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
        Clipboard, ClipboardContent, ClipboardContext, RustImageData, common::RustImage,
    };
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::Foundation::HANDLE,
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
    let status = |expected_paste: bool| -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match events.try_recv() {
                Ok(Event::Copied { for_paste, .. }) => {
                    assert_eq!(for_paste, expected_paste);
                    return Ok(());
                }
                Ok(Event::Error(message)) => bail!("{message}"),
                _ => {}
            }
            if Instant::now() >= deadline {
                bail!("等待复制确认超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };

    let text = format!("isolated-text-{}", std::process::id());
    clipboard
        .set_text(text.clone())
        .map_err(|error| anyhow!("写入测试文本失败：{error}"))?;
    let text_id = snapshot("text")?;
    service.send(Command::CopyForPaste(text_id))?;
    status(true)?;
    assert_eq!(
        clipboard
            .get_text()
            .map_err(|error| anyhow!("读回文本失败：{error}"))?,
        text
    );
    println!("text capture/copy ok");

    let html = "<b>rich smoke</b>";
    clipboard
        .set(vec![
            ClipboardContent::Text("rich smoke".into()),
            ClipboardContent::Html(html.into()),
            ClipboardContent::Other("Rich Text Format".into(), b"{\\rtf1 rich smoke}\0".to_vec()),
        ])
        .map_err(|error| anyhow!("写入测试富文本失败：{error}"))?;
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
                    PreviewContent::RichText("rich smoke".into())
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
    assert!(
        clipboard
            .get_buffer("Rich Text Format")
            .map_err(|error| anyhow!("读回 RTF 失败：{error}"))?
            .starts_with(b"{\\rtf1")
    );
    println!("rich capture/copy ok");

    let image_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clipboard-rs/tests/test.png");
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
        vec![file]
    );
    println!("files capture/copy ok");

    drop(service);
    println!("isolated clipboard roundtrip passed");
    Ok(())
}
