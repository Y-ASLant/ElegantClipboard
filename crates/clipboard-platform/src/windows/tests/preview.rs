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
fn hover_preview_has_its_own_response_and_reads_full_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let text = format!("悬停预览\n{}", "正文".repeat(500));
    service.send(Command::Capture(text.clone()))?;
    let id = next_snapshot(&events, 0)[0].id;
    service.send(Command::HoverPreview { id, generation: 7 })?;
    service.send(Command::Preview { id, generation: 3 })?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut hover_seen = false;
    let mut preview_seen = false;
    while !hover_seen || !preview_seen {
        match events.try_recv() {
            Ok(Event::HoverPreview {
                id: actual,
                generation,
                result,
            }) => {
                assert_eq!((actual, generation), (id, 7));
                assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                hover_seen = true;
            }
            Ok(Event::Preview {
                id: actual,
                generation,
                result,
            }) => {
                assert_eq!((actual, generation), (id, 3));
                assert_eq!(result.unwrap(), PreviewContent::Text(text.clone()));
                preview_seen = true;
            }
            Ok(Event::Error(message)) => panic!("{message}"),
            _ => {}
        }
        assert!(std::time::Instant::now() < deadline, "preview timed out");
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
