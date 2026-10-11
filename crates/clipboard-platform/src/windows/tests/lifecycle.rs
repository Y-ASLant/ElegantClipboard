use super::super::{FailureKind, show_existing_instance};
use super::*;
use clipboard_core::preferences::Preferences;

#[test]
fn onboarding_completion_and_existing_history_are_recognized() -> Result<()> {
    let fresh = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(fresh.path().to_owned()), false)?;
    assert!(!service.initial_onboarding_completed);
    next_snapshot(&events, 0);
    service.send(Command::CompleteOnboarding)?;
    assert!(matches!(
        next_event(&events),
        Event::OnboardingCompleted(Ok(()))
    ));
    drop(service);
    let (service, _) = Service::start(Some(fresh.path().to_owned()), false)?;
    assert!(service.initial_onboarding_completed);
    drop(service);

    let existing = tempfile::tempdir()?;
    let history = History::open(existing.path().join("clipboard.db"))?;
    let id = history.capture("existing history")?.expect("new item");
    let group = history.create_group("Existing group")?;
    history.move_to_group(id, None, Some(group.id))?;
    assert_eq!(history.count("", false)?, 0);
    drop(history);
    let (service, _) = Service::start(Some(existing.path().to_owned()), false)?;
    assert!(service.initial_onboarding_completed);
    drop(service);
    let db = Database::new(existing.path().join("clipboard.db"))?;
    assert!(Preferences::new(&db).onboarding_completed()?);
    Ok(())
}

#[test]
fn invalid_history_row_reports_startup_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let id = History::new(&db).capture("invalid row")?.unwrap();
    db.write_connection().lock().execute(
        &format!("UPDATE clipboard_items SET byte_size = X'01' WHERE id = {id}"),
        [],
    )?;
    drop(db);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "启动错误未上报",
        |event| match event {
            Event::Error(message) => {
                assert!(message.contains("读取历史记录失败"), "{message}");
                Some(())
            }
            Event::Snapshot { .. } => panic!("损坏记录被错误地显示为正常快照"),
            _ => None,
        },
    );
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 7,
    })?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "查询错误未携带请求代次",
        |event| match event {
            Event::CommandFailed {
                kind: FailureKind::Query(7),
                message,
            } => {
                assert!(message.contains("读取历史记录失败"), "{message}");
                Some(())
            }
            Event::Snapshot { .. } => panic!("损坏记录被错误地显示为正常快照"),
            _ => None,
        },
    );
    Ok(())
}

#[test]
fn instance_lock_and_shutdown_work_even_with_a_full_event_queue() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(Service::start(Some(directory.path().to_owned()), false).is_err());
    next_snapshot(&events, 0);
    for _ in 0..64 {
        assert!(
            service
                .events
                .try_send(Event::Status(String::new()))
                .is_ok()
        );
    }
    service.send(Command::Capture("shutdown barrier".into()))?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while history.count("shutdown barrier", false)? != 1 {
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not reach its blocked snapshot"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(show_existing_instance(Some(directory.path().to_owned()))?);
    service
        .instance_signal
        .as_ref()
        .unwrap()
        .wake_received
        .recv_timeout(Duration::from_secs(5))?;
    drop(service);
    let (service, _) = Service::start(Some(directory.path().to_owned()), false)?;
    drop(service);
    Ok(())
}

#[test]
fn second_launch_signal_is_scoped_to_data_directory() -> Result<()> {
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    assert!(!show_existing_instance(Some(first.path().to_owned()))?);
    let (service, events) = Service::start(Some(first.path().to_owned()), false)?;
    assert!(show_existing_instance(Some(first.path().to_owned()))?);
    assert!(!show_existing_instance(Some(second.path().to_owned()))?);
    wait_for(
        &events,
        Duration::from_secs(2),
        "second-launch wake request was lost",
        |event| matches!(event, Event::ShowWindow).then_some(()),
    );
    drop(service);
    assert!(!show_existing_instance(Some(first.path().to_owned()))?);
    Ok(())
}

#[test]
fn second_launch_wakes_after_a_full_event_queue_drains() -> Result<()> {
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
    assert!(show_existing_instance(Some(directory.path().to_owned()))?);
    service
        .instance_signal
        .as_ref()
        .unwrap()
        .wake_received
        .recv_timeout(Duration::from_secs(5))?;
    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::Status(_)));
    }
    wait_for(
        &events,
        Duration::from_secs(2),
        "second-launch wake request was lost",
        |event| matches!(event, Event::ShowWindow).then_some(()),
    );
    Ok(())
}
