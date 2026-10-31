//! 仅由已切换到独立窗口站的主程序调用，不读取或写入交互桌面剪贴板。
use anyhow::{Context, Result, anyhow, bail};
use clipboard_core::{History, database::ClipboardItem};
use clipboard_platform::{Command, CopyMode, CopyPurpose, Event, Service};
use clipboard_rs::{
    Clipboard, ClipboardContent, ClipboardContext, RustImageData, common::RustImage,
};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub(super) fn run(
    service: &Service,
    events: &async_channel::Receiver<Event>,
    clipboard: &ClipboardContext,
    data_dir: &Path,
) -> Result<()> {
    let history = || History::open(data_dir.join("clipboard.db"));
    let wait_item = |predicate: &dyn Fn(&ClipboardItem) -> bool| -> Result<ClipboardItem> {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(item) = history()?
                .list("", 1000, false)?
                .into_iter()
                .find(predicate)
            {
                return Ok(item);
            }
            if Instant::now() >= deadline {
                bail!("等待跨平台回归样本入库超时");
            }
            thread::sleep(Duration::from_millis(10));
        }
    };

    // 无效字节偏移不应使 watcher 持锁 panic；同一次复制仍可保留其纯文本。
    let header = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\n";
    let start = header.len();
    let malformed = format!(
        "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{:010}\r\n中\0",
        start + 1
    );
    clipboard
        .set(vec![
            ClipboardContent::Text("offset boundary fallback".into()),
            ClipboardContent::Other("HTML Format".into(), malformed.into_bytes()),
        ])
        .map_err(|error| anyhow!("写入隔离 HTML 边界样本失败：{error}"))?;
    wait_item(&|item| item.preview.as_deref() == Some("offset boundary fallback"))?;
    clipboard
        .set_text("watcher survived invalid HTML".into())
        .map_err(|error| anyhow!("写入隔离监听存活样本失败：{error}"))?;
    wait_item(&|item| item.preview.as_deref() == Some("watcher survived invalid HTML"))?;
    println!("invalid HTML boundary and watcher recovery ok");

    let png = encoded_image(2, 2)?;
    clipboard
        .set(vec![
            ClipboardContent::Html(
                "<html><body><img src='blob:isolated-regression'></body></html>".into(),
            ),
            ClipboardContent::Image(
                RustImageData::from_bytes(&png).map_err(|error| anyhow!("{error}"))?,
            ),
        ])
        .map_err(|error| anyhow!("写入隔离单图 HTML 失败：{error}"))?;
    let item = wait_item(&|item| {
        item.content_type == "image" && item.image_width == Some(2) && item.image_height == Some(2)
    })?;
    let path = item.image_path.as_deref().context("图片未保存为像素附件")?;
    // 外部编辑原附件后，写回必须使用解码后的实际尺寸而不是数据库旧尺寸。
    std::fs::write(path, encoded_image(1, 1)?)?;
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::Original,
        purpose: CopyPurpose::Copy,
    })?;
    copied(events, CopyPurpose::Copy)?;
    let dib: Vec<u8> = clipboard_win::get_clipboard(clipboard_win::formats::RawData(8))
        .map_err(|error| anyhow!("读回隔离 DIB 失败：{error}"))?;
    assert_eq!(i32::from_le_bytes(dib[4..8].try_into()?), 1);
    assert_eq!(i32::from_le_bytes(dib[8..12].try_into()?), 1);
    println!("image clipboard uses decoded dimensions ok");

    let text = "native payload original";
    let native = "ElegantClipboard Regression Native";
    clipboard
        .set(vec![
            ClipboardContent::Text(text.into()),
            ClipboardContent::Other(native.into(), b"original structured payload".to_vec()),
        ])
        .map_err(|error| anyhow!("写入隔离原生格式失败：{error}"))?;
    let item = wait_item(&|item| item.preview.as_deref() == Some(text))?;
    assert!(
        history()?
            .native_formats_for_copy(item.id)?
            .iter()
            .any(|(name, _)| name == native)
    );
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::PlainText,
        purpose: CopyPurpose::Copy,
    })?;
    copied(events, CopyPurpose::Copy)?;
    assert!(
        !clipboard
            .available_formats()
            .map_err(|error| anyhow!("{error}"))?
            .iter()
            .any(|name| name == native)
    );
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::Transform(clipboard_core::TextTransform::Uppercase),
        purpose: CopyPurpose::Copy,
    })?;
    copied(events, CopyPurpose::Copy)?;
    assert_eq!(
        clipboard.get_text().map_err(|error| anyhow!("{error}"))?,
        text.to_uppercase()
    );
    assert!(
        !clipboard
            .available_formats()
            .map_err(|error| anyhow!("{error}"))?
            .iter()
            .any(|name| name == native)
    );
    println!("plain and transformed copies exclude native payloads ok");

    // 失败请求的快照不能由后一次纯文本粘贴取走；外部新复制也不能被恢复覆盖。
    service.send(Command::SetRestoreClipboard(true))?;
    wait_event(events, &|event| {
        matches!(event, Event::RestoreClipboardSaved(Ok(true)))
    })?;
    clipboard
        .set_text("original stash".into())
        .map_err(|error| anyhow!("{error}"))?;
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::Original,
        purpose: CopyPurpose::Paste { request_id: 8001 },
    })?;
    copied(events, CopyPurpose::Paste { request_id: 8001 })?;
    service.send(Command::CancelPaste { request_id: 8001 })?;
    clipboard
        .set_text("new user copy".into())
        .map_err(|error| anyhow!("{error}"))?;
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::PlainText,
        purpose: CopyPurpose::Paste { request_id: 8002 },
    })?;
    let sequence = copied(events, CopyPurpose::Paste { request_id: 8002 })?;
    service.send(Command::RestoreStashedClipboard {
        request_id: 8002,
        clipboard_sequence: sequence,
    })?;
    wait_event(events, &|event| {
        matches!(event, Event::ClipboardRestored(Ok(false)))
    })?;
    assert_eq!(
        clipboard.get_text().map_err(|error| anyhow!("{error}"))?,
        text
    );
    println!("cancelled request cannot restore into a later paste ok");

    clipboard
        .set_text("matching request stash".into())
        .map_err(|error| anyhow!("{error}"))?;
    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::Original,
        purpose: CopyPurpose::Paste { request_id: 8003 },
    })?;
    let sequence = copied(events, CopyPurpose::Paste { request_id: 8003 })?;
    service.send(Command::RestoreStashedClipboard {
        request_id: 8003,
        clipboard_sequence: sequence,
    })?;
    wait_event(events, &|event| {
        matches!(event, Event::ClipboardRestored(Ok(true)))
    })?;
    assert_eq!(
        clipboard.get_text().map_err(|error| anyhow!("{error}"))?,
        "matching request stash"
    );
    println!("matching clipboard restoration ok");

    service.send(Command::Copy {
        id: item.id,
        mode: CopyMode::Original,
        purpose: CopyPurpose::Paste { request_id: 8004 },
    })?;
    let sequence = copied(events, CopyPurpose::Paste { request_id: 8004 })?;
    clipboard
        .set_text("external content stays".into())
        .map_err(|error| anyhow!("{error}"))?;
    service.send(Command::RestoreStashedClipboard {
        request_id: 8004,
        clipboard_sequence: sequence,
    })?;
    wait_event(events, &|event| {
        matches!(event, Event::ClipboardRestored(Ok(false)))
    })?;
    assert_eq!(
        clipboard.get_text().map_err(|error| anyhow!("{error}"))?,
        "external content stays"
    );
    println!("external clipboard content survives restoration request ok");
    service.send(Command::SetRestoreClipboard(false))?;
    wait_event(events, &|event| {
        matches!(event, Event::RestoreClipboardSaved(Ok(false)))
    })?;
    Ok(())
}

fn encoded_image(width: u32, height: u32) -> Result<Vec<u8>> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([17, 67, 131, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

fn copied(events: &async_channel::Receiver<Event>, expected_purpose: CopyPurpose) -> Result<u32> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(Event::Copied {
                purpose,
                clipboard_sequence,
                ..
            }) if purpose == expected_purpose => return Ok(clipboard_sequence),
            Ok(Event::CommandFailed { message, .. } | Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        thread::sleep(Duration::from_millis(10));
        if Instant::now() >= deadline {
            bail!("等待隔离复制回执超时");
        }
    }
}

fn wait_event(
    events: &async_channel::Receiver<Event>,
    matches: &dyn Fn(&Event) -> bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match events.try_recv() {
            Ok(event) if matches(&event) => return Ok(()),
            Ok(Event::CommandFailed { message, .. } | Event::Error(message)) => bail!("{message}"),
            _ => {}
        }
        if Instant::now() >= deadline {
            bail!("等待隔离回归回执超时");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
