use super::super::{CapturedClipboard, FailureKind};
use super::*;
use clipboard_core::{
    MAX_TEXT_BYTES,
    preferences::{MonitorTypesPreference, ThemePreference},
};

#[test]
fn command_failures_wait_for_room_in_a_full_event_queue() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    for _ in 0..64 {
        assert!(
            service
                .events
                .try_send(Event::Status(String::new()))
                .is_ok()
        );
    }
    service.send(Command::CopyForPaste(424242, 1))?;
    service.send(Command::CopyPathForPaste(424243, 2))?;
    service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;

    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::Status(_)));
    }
    let mut failures = Vec::new();
    let mut capture_error = false;
    wait_for(
        &events,
        Duration::from_secs(5),
        "worker did not report all command results",
        |event| {
            match event {
                Event::CommandFailed { kind, .. } => failures.push(kind),
                Event::BackgroundError(_) => capture_error = true,
                Event::ThemeSaved(Ok(ThemePreference::Dark)) => return Some(()),
                _ => {}
            }
            None
        },
    );
    assert_eq!(failures, [FailureKind::Paste(1), FailureKind::Paste(2)]);
    assert!(capture_error, "explicit capture failure was dropped");
    Ok(())
}

fn fill_event_queue(service: &Service) {
    for _ in 0..64 {
        assert!(service.events.try_send(Event::ShowWindow).is_ok());
    }
}

fn enqueue_observed_text(service: &Service, text: String) -> Result<()> {
    service.send(Command::ObservedCapture {
        content: CapturedClipboard::Text(text),
        source: None,
    })
}

#[test]
fn captures_persist_while_ui_events_are_full_and_refresh_after_drain() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    fill_event_queue(&service);
    for i in 0..128 {
        enqueue_observed_text(&service, format!("buffered-{i}"))?;
    }
    let history = History::open(directory.path().join("clipboard.db"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while history.count("", false)? != 128 {
        assert!(
            std::time::Instant::now() < deadline,
            "UI congestion stopped persistence"
        );
        thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::ShowWindow));
    }
    wait_for(
        &events,
        Duration::from_secs(5),
        "deferred snapshot was not delivered",
        |event| matches!(event, Event::Snapshot { total: 128, .. }).then_some(()),
    );
    Ok(())
}

#[test]
fn accepted_captures_drain_on_shutdown_even_with_a_blocked_reply() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    fill_event_queue(&service);
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    for i in 0..128 {
        enqueue_observed_text(&service, format!("shutdown-{i}"))?;
    }
    drop(service);
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert_eq!(history.count("", false)?, 128);
    Ok(())
}

#[test]
fn shutdown_drain_preserves_filter_changes_before_accepted_captures() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    fill_event_queue(&service);
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    enqueue_observed_text(&service, "before-filter".into())?;
    service.send(Command::SetMonitorTypes(MonitorTypesPreference {
        text: false,
        ..Default::default()
    }))?;
    enqueue_observed_text(&service, "excluded-after-filter".into())?;
    drop(service);
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert_eq!(history.count("", false)?, 1);
    assert_eq!(history.count("before-filter", false)?, 1);
    Ok(())
}

#[test]
fn buffered_captures_keep_filter_command_order_after_a_blocked_reply() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    fill_event_queue(&service);
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    for i in 0..128 {
        enqueue_observed_text(&service, format!("before-filter-{i}"))?;
    }
    service.send(Command::SetMonitorTypes(MonitorTypesPreference {
        text: false,
        ..Default::default()
    }))?;
    enqueue_observed_text(&service, "after-filter".into())?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: 1000,
        generation: 1,
    })?;
    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::ShowWindow));
    }
    let rows = next_snapshot(&events, 1);
    assert_eq!(rows.len(), 128);
    assert!(
        rows.iter()
            .all(|item| item.preview.as_ref().unwrap().starts_with("before-filter-"))
    );
    Ok(())
}

#[test]
fn observed_capture_failure_is_reported_after_full_ui_queue_drains() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    fill_event_queue(&service);
    enqueue_observed_text(&service, "x".repeat(MAX_TEXT_BYTES + 1))?;
    enqueue_observed_text(&service, "saved-after-failure".into())?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while history.count("", false)? != 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "capture worker stopped after failure"
        );
        thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::ShowWindow));
    }
    wait_for(
        &events,
        Duration::from_secs(5),
        "capture error was dropped",
        |event| {
            matches!(event, Event::BackgroundError(message) if message.contains("1 MiB"))
                .then_some(())
        },
    );
    Ok(())
}
