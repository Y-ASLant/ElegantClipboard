use super::super::{CapturedClipboard, FailureKind, worker::filter_observed_capture};
use super::*;
use crate::source_app;
use anyhow::{Context, anyhow};
use clipboard_core::{
    FilePreviewEntry, PreviewContent,
    preferences::{AppFilterPreference, MonitorTypesPreference, Preferences, ThemePreference},
};
use clipboard_rs::{RustImageData, common::RustImage};
use std::path::{Path, PathBuf};

#[test]
fn observed_capture_filter_distinguishes_url_and_preserves_allowed_rich_format() {
    let allowed = MonitorTypesPreference {
        text: false,
        url: true,
        html: false,
        rtf: true,
        image: false,
        files: false,
    };
    assert!(
        filter_observed_capture(CapturedClipboard::Text("plain text".into()), allowed).is_none()
    );
    assert!(matches!(
        filter_observed_capture(
            CapturedClipboard::Text("https://example.com".into()),
            allowed
        ),
        Some(CapturedClipboard::Text(_))
    ));
    assert!(matches!(
        filter_observed_capture(
            CapturedClipboard::Rich {
                html: Some("<b>rich</b>".into()),
                rtf: Some(b"{\\rtf1 rich}".to_vec()),
                text: Some("rich".into()),
            },
            allowed
        ),
        Some(CapturedClipboard::Rich {
            html: None,
            rtf: Some(_),
            ..
        })
    ));
    assert!(
        filter_observed_capture(
            CapturedClipboard::Files(vec!["C:\\test.txt".into()]),
            allowed
        )
        .is_none()
    );
    assert!(
        filter_observed_capture(
            CapturedClipboard::Image {
                png: vec![1],
                width: 1,
                height: 1
            },
            allowed
        )
        .is_none()
    );
}

#[test]
fn monitor_types_change_applies_to_later_observed_captures() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let allowed = MonitorTypesPreference {
        text: false,
        ..MonitorTypesPreference::default()
    };
    service.send(Command::SetMonitorTypes(allowed))?;
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("ordinary text".into()),
        source: None,
    })?;
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("https://example.com".into()),
        source: None,
    })?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    let items = next_snapshot(&events, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].preview.as_deref(), Some("https://example.com"));
    drop(service);
    let db = Database::new(directory.path().join("clipboard.db"))?;
    assert_eq!(Preferences::new(&db).monitor_types()?, allowed);
    Ok(())
}

#[test]
fn app_filter_changes_apply_to_observed_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let blacklist = AppFilterPreference {
        enabled: true,
        mode: clipboard_core::preferences::AppFilterMode::Blacklist,
        rules: vec!["notepad.exe".into()],
    };
    service.send(Command::SetAppFilter(Box::new(blacklist.clone())))?;
    let notepad = source_app::SourceApp {
        name: "Notepad".into(),
        executable: Some("C:\\Windows\\notepad.exe".into()),
    };
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("blocked note".into()),
        source: Some(notepad.clone()),
    })?;
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("unknown source".into()),
        source: None,
    })?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    let items = next_snapshot(&events, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].preview.as_deref(), Some("unknown source"));

    let whitelist = AppFilterPreference {
        mode: clipboard_core::preferences::AppFilterMode::Whitelist,
        ..blacklist
    };
    service.send(Command::SetAppFilter(Box::new(whitelist.clone())))?;
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("allowed note".into()),
        source: Some(notepad),
    })?;
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("blocked browser".into()),
        source: Some(source_app::SourceApp {
            name: "Browser".into(),
            executable: Some("C:\\Apps\\browser.exe".into()),
        }),
    })?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    let items = next_snapshot(&events, 2);
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .any(|item| item.preview.as_deref() == Some("allowed note"))
    );
    assert!(
        !items
            .iter()
            .any(|item| item.preview.as_deref() == Some("blocked browser"))
    );
    drop(service);
    let db = Database::new(directory.path().join("clipboard.db"))?;
    assert_eq!(Preferences::new(&db).app_filter()?, whitelist);
    Ok(())
}

#[test]
fn observed_captures_keep_the_source_with_each_record() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);

    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("source test".into()),
        source: Some(source_app::SourceApp {
            name: "Editor".into(),
            executable: None,
        }),
    })?;
    let items = next_snapshot(&events, 0);
    assert_eq!(items[0].source_app_name.as_deref(), Some("Editor"));

    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("source test".into()),
        source: Some(source_app::SourceApp {
            name: "Browser".into(),
            executable: None,
        }),
    })?;
    let items = next_snapshot(&events, 0);
    assert_eq!(items[0].source_app_name.as_deref(), Some("Browser"));

    for (content, source) in [
        (
            CapturedClipboard::Rich {
                html: Some("<b>rich source</b>".into()),
                rtf: None,
                text: Some("rich source".into()),
            },
            "Mail",
        ),
        (
            CapturedClipboard::Files(vec![r"C:\source-test.txt".into()]),
            "Explorer",
        ),
        (
            CapturedClipboard::Image {
                png: include_bytes!("../../../tests/fixtures/test.png").to_vec(),
                width: 128,
                height: 128,
            },
            "Paint",
        ),
    ] {
        service.send(Command::ObservedCapture {
            content,
            source: Some(source_app::SourceApp {
                name: source.into(),
                executable: None,
            }),
        })?;
        let items = next_snapshot(&events, 0);
        assert_eq!(items[0].source_app_name.as_deref(), Some(source));
    }
    Ok(())
}

#[test]
#[ignore = "requires a Windows system executable with an icon"]
fn observed_capture_persists_a_cached_source_icon() -> Result<()> {
    let system_root = std::env::var("SystemRoot")?;
    let executable = Path::new(&system_root).join("System32/notepad.exe");
    anyhow::ensure!(executable.is_file(), "system executable is unavailable");
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text("icon source test".into()),
        source: Some(source_app::SourceApp {
            name: "Notepad".into(),
            executable: Some(executable.to_string_lossy().into_owned()),
        }),
    })?;
    let items = next_snapshot(&events, 0);
    assert_eq!(items[0].source_app_name.as_deref(), Some("Notepad"));
    let icon = items[0]
        .source_app_icon
        .as_deref()
        .context("missing icon path")?;
    assert!(Path::new(icon).starts_with(directory.path().join("icons")));
    assert!(Path::new(icon).is_file());
    Ok(())
}

#[test]
fn image_capture_is_persisted_and_delete_removes_managed_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let png = include_bytes!("../../../tests/fixtures/test.png");
    service.send(Command::CaptureImage {
        png: png.to_vec(),
        width: 128,
        height: 128,
    })?;
    let rows = next_snapshot(&events, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content_type, "image");
    let image_path = PathBuf::from(rows[0].image_path.as_ref().unwrap());
    assert_eq!(std::fs::read(&image_path)?, png);
    let image = RustImageData::from_path(image_path.to_str().unwrap())
        .map_err(|error| anyhow!("打开测试图片失败：{error}"))?;
    assert_eq!(image.get_size(), (128, 128));
    service.send(Command::Delete(rows[0].id))?;
    assert!(next_snapshot(&events, 0).is_empty());
    assert!(!image_path.exists());
    Ok(())
}

#[test]
fn file_paths_are_saved_and_missing_sources_do_not_touch_clipboard() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("sample.txt");
    std::fs::write(&path, "sample")?;
    let path = path.to_string_lossy().into_owned();
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::CaptureFiles(vec![path.clone()]))?;
    let rows = next_snapshot(&events, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content_type, "files");
    let id = rows[0].id;
    service.send(Command::Preview { id, generation: 1 })?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "preview timed out",
        |event| match event {
            Event::Preview { result, .. } => {
                assert_eq!(
                    result.unwrap(),
                    PreviewContent::Files(vec![FilePreviewEntry {
                        original_path: path.clone(),
                        resolved_path: path.clone(),
                        exists: true,
                        is_dir: false,
                        size: Some(6),
                        recovered: false,
                        metadata_error: None,
                    }])
                );
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    std::fs::remove_file(path)?;
    service.send(Command::Copy(id))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match events.try_recv() {
            Ok(Event::CommandFailed {
                kind: FailureKind::Other,
                message,
            }) => {
                assert!(message.contains("已不存在"));
                break;
            }
            Ok(Event::Error(message) | Event::BackgroundError(message)) => bail!("{message}"),
            _ => {}
        }
        assert!(std::time::Instant::now() < deadline, "copy error timed out");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(
        events.recv_blocking()?,
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn rich_capture_keeps_html_and_binary_rtf_for_preview() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let rtf = b"{\\rtf1\\bin2 \x00\x01}".to_vec();
    service.send(Command::CaptureRich {
        html: Some("<b>中文😀</b>".into()),
        rtf: Some(rtf.clone()),
        text: Some("中文😀".into()),
    })?;
    let rows = next_snapshot(&events, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content_type, "html");
    let stored = History::open(directory.path().join("clipboard.db"))?.item(rows[0].id)?;
    assert_eq!(stored.html_content.as_deref(), Some("<b>中文😀</b>"));
    assert_eq!(
        stored
            .rtf_content
            .as_deref()
            .and_then(clipboard_core::rich::decode_rtf_for_clipboard)
            .unwrap(),
        [rtf, vec![0]].concat()
    );
    service.send(Command::Preview {
        id: rows[0].id,
        generation: 1,
    })?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "preview timed out",
        |event| match event {
            Event::Preview { result, .. } => {
                assert_eq!(result.unwrap(), PreviewContent::RichText("中文😀".into()));
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    Ok(())
}
