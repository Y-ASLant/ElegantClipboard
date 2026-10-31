use super::*;
use clipboard_core::PreviewContent;

#[test]
fn preview_returns_full_text_and_reports_deleted_records() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let text = format!("中文😀\r\n{}\n末尾", "long text ".repeat(600));
    service.send(Command::Capture(text.clone()))?;
    let rows = next_snapshot(&events, 0);
    let id = rows[0].id;
    assert!(rows[0].text_content.is_none());
    for generation in 1..=2 {
        if generation == 2 {
            service.send(Command::Delete(id))?;
            assert!(next_snapshot(&events, 0).is_empty());
        }
        service.send(Command::Preview { id, generation })?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "preview timed out",
            |event| match event {
                Event::Preview {
                    id: actual,
                    generation: request,
                    result,
                } => {
                    assert_eq!((actual, request), (id, generation));
                    if generation == 1 {
                        assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                    } else {
                        assert!(result.is_err());
                    }
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    Ok(())
}

#[test]
fn edit_content_reads_stored_text_without_changing_html_or_hover_preview() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let html = "<b>formatted</b>";
    let text = "stored plain text\n中文😀";
    service.send(Command::CaptureRich {
        html: Some(html.into()),
        rtf: Some(b"{\\rtf1 formatted}".to_vec()),
        text: Some(text.into()),
    })?;
    let id = next_snapshot(&events, 0)[0].id;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let before = history.item(id)?;
    service.send(Command::EditContent { id, generation: 11 })?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "edit content timed out",
        |event| match event {
            Event::Preview {
                id: actual,
                generation,
                result,
            } => {
                assert_eq!((actual, generation), (id, 11));
                assert_eq!(result.unwrap(), PreviewContent::RichText(text.into()));
                Some(())
            }
            Event::Error(message) | Event::CommandFailed { message, .. } => panic!("{message}"),
            _ => None,
        },
    );
    service.send(Command::HoverPreview { id, generation: 12 })?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hover preview timed out",
        |event| match event {
            Event::HoverPreview {
                id: actual,
                generation,
                result,
            } => {
                assert_eq!((actual, generation), (id, 12));
                assert_eq!(result.unwrap(), PreviewContent::Html(html.into()));
                Some(())
            }
            Event::Error(message) | Event::CommandFailed { message, .. } => panic!("{message}"),
            _ => None,
        },
    );
    let after = history.item(id)?;
    assert_eq!(after.content_type, before.content_type);
    assert_eq!(after.content_hash, before.content_hash);
    assert_eq!(after.text_content, before.text_content);
    assert_eq!(after.html_content, before.html_content);
    assert_eq!(after.rtf_content, before.rtf_content);
    Ok(())
}

#[test]
fn hover_preview_has_its_own_response_and_reads_full_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let text = format!("悬停预览\n{}", "正文".repeat(500));
    service.send(Command::Capture(text.clone()))?;
    let id = next_snapshot(&events, 0)[0].id;
    service.send(Command::HoverPreview { id, generation: 7 })?;
    service.send(Command::Preview { id, generation: 3 })?;
    let mut hover_seen = false;
    let mut preview_seen = false;
    wait_for(
        &events,
        Duration::from_secs(5),
        "preview timed out",
        |event| {
            match event {
                Event::HoverPreview {
                    id: actual,
                    generation,
                    result,
                } => {
                    assert_eq!((actual, generation), (id, 7));
                    assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                    hover_seen = true;
                }
                Event::Preview {
                    id: actual,
                    generation,
                    result,
                } => {
                    assert_eq!((actual, generation), (id, 3));
                    assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                    preview_seen = true;
                }
                Event::Error(message) => panic!("{message}"),
                _ => {}
            }
            (hover_seen && preview_seen).then_some(())
        },
    );
    Ok(())
}
