use super::*;
use clipboard_core::database::GroupRepository;

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
fn onboarding_completion_and_existing_history_are_recognized() -> Result<()> {
    let fresh = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(fresh.path().to_owned()), false)?;
    assert!(!service.initial_onboarding_completed);
    next_snapshot(&events, 0);
    service.send(Command::CompleteOnboarding)?;
    assert!(matches!(
        events.recv_blocking()?,
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
fn plain_text_paste_failure_is_routed_to_its_pending_item() {
    assert_eq!(
        Command::CopyPlainTextForPaste(42).failure_kind(),
        Some(FailureKind::Paste(42))
    );
    assert_eq!(
        Command::CopyPlainText(42).failure_kind(),
        Some(FailureKind::Other)
    );
}

#[test]
fn plain_text_copy_uses_only_stored_text_and_rejects_missing_representation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let rich = history.capture_rich(Some("<b>格式</b>"), None, Some("格式"), &images)?;
    assert_eq!(plain_text_for_copy(&history.item(rich)?)?, "格式");
    let no_text = history.capture_rich(Some("<i>only html</i>"), None, None, &images)?;
    assert!(plain_text_for_copy(&history.item(no_text)?).is_err());
    let file = history.capture_files(&["C:\\example.txt".into()], &images)?;
    assert!(plain_text_for_copy(&history.item(file)?).is_err());
    Ok(())
}

#[test]
fn file_path_actions_use_resolved_existing_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let source = directory.path().join("中文 file.txt");
    std::fs::write(&source, "content")?;
    let id = history.capture_files(
        &[source.to_string_lossy().into_owned()],
        &directory.path().join("images"),
    )?;
    assert_eq!(
        item_paths_for_action(&history.item(id)?, &directory.path().join("staged"))?,
        vec![source.to_string_lossy().into_owned()]
    );

    let command = explorer_command(&source)?;
    assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("/select,"), source.as_os_str()]
    );

    std::fs::remove_file(&source)?;
    assert!(item_paths_for_action(&history.item(id)?, &directory.path().join("staged")).is_err());
    let text = history.capture("not a file")?.unwrap();
    assert!(item_paths_for_action(&history.item(text)?, &directory.path().join("staged")).is_err());
    Ok(())
}

#[test]
fn save_as_copies_the_first_resolved_file_and_rejects_unsafe_targets() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let staged = directory.path().join("staged");
    let source = directory.path().join("中文 source.txt");
    std::fs::write(&source, "saved content")?;
    let file_id = history.capture_files(&[source.to_string_lossy().into_owned()], &images)?;
    let destination = directory.path().join("另存 file.txt");
    assert_eq!(
        save_item_as(&history.item(file_id)?, &staged, &destination)?,
        13
    );
    assert_eq!(std::fs::read_to_string(&destination)?, "saved content");
    assert!(save_item_as(&history.item(file_id)?, &staged, &source).is_err());

    let folder = directory.path().join("folder");
    std::fs::create_dir(&folder)?;
    let folder_id = history.capture_files(&[folder.to_string_lossy().into_owned()], &images)?;
    assert!(
        save_item_as(
            &history.item(folder_id)?,
            &staged,
            &directory.path().join("folder-copy")
        )
        .is_err()
    );

    let image_id = history.capture_image(b"\x89PNG\r\n\x1a\nsynthetic", 1, 1, &images)?;
    let image_destination = directory.path().join("image-copy.png");
    save_item_as(&history.item(image_id)?, &staged, &image_destination)?;
    assert_eq!(
        std::fs::read(&image_destination)?,
        b"\x89PNG\r\n\x1a\nsynthetic"
    );
    Ok(())
}

#[test]
fn data_size_counts_database_and_managed_media() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let images = directory.path().join("images");
    let nested_images = images.join("nested");
    let staged = directory.path().join("staged");
    std::fs::create_dir_all(&nested_images)?;
    std::fs::create_dir_all(&staged)?;
    std::fs::write(directory.path().join("clipboard.db"), [0u8; 3])?;
    std::fs::write(directory.path().join("clipboard.db-wal"), [0u8; 5])?;
    std::fs::write(images.join("one.png"), [0u8; 7])?;
    std::fs::write(nested_images.join("two.png"), [0u8; 11])?;
    std::fs::write(staged.join("payload.bin"), [0u8; 13])?;
    std::fs::write(directory.path().join("unmanaged.txt"), [0u8; 17])?;

    assert_eq!(
        data_size_info(directory.path(), &images, &staged)?,
        DataSizeInfo {
            database_bytes: 8,
            image_bytes: 18,
            image_count: 2,
            staged_bytes: 13,
            staged_count: 1,
            total_bytes: 39,
        }
    );
    let command = data_directory_command(directory.path())?;
    assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
    assert_eq!(command.get_args().collect::<Vec<_>>(), [directory.path()]);
    Ok(())
}

#[test]
fn database_maintenance_preserves_history_and_returns_updated_size() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("clipboard.db");
    let history = History::open(database.clone())?;
    let id = history.capture("keep after maintenance")?.unwrap();
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::OptimizeDatabase)?;
    match events.recv_blocking()? {
        Event::DatabaseOptimized(size) => {
            assert!(size.database_bytes > 0);
            assert_eq!(size.total_bytes, size.database_bytes);
        }
        Event::CommandFailed { message, .. } | Event::Error(message) => panic!("{message}"),
        _ => panic!("unexpected database maintenance event"),
    }
    drop(service);

    assert_eq!(History::open(database)?.text(id)?, "keep after maintenance");
    Ok(())
}

#[test]
fn failed_paste_copy_identifies_only_its_own_request() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::CopyForPaste(424242))?;
    service.send(Command::CopyPathForPaste(424243))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::CommandFailed {
            kind: FailureKind::Paste(424242),
            ..
        }
    ));
    assert!(matches!(
        events.recv_blocking()?,
        Event::CommandFailed {
            kind: FailureKind::Paste(424243),
            ..
        }
    ));
    assert!(matches!(
        events.recv_blocking()?,
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn failed_merge_identifies_only_the_batch_request() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::MergeForPaste(vec![424242, 424243]))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::CommandFailed {
            kind: FailureKind::Merge,
            ..
        }
    ));
    assert!(matches!(
        events.recv_blocking()?,
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

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
    service.send(Command::CopyForPaste(424242))?;
    service.send(Command::CopyPathForPaste(424243))?;
    service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;

    for _ in 0..64 {
        assert!(matches!(events.try_recv()?, Event::Status(_)));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut failures = Vec::new();
    let mut capture_error = false;
    let mut theme_saved = false;
    while !theme_saved {
        match events.try_recv() {
            Ok(Event::CommandFailed { kind, .. }) => failures.push(kind),
            Ok(Event::BackgroundError(_)) => capture_error = true,
            Ok(Event::ThemeSaved(Ok(ThemePreference::Dark))) => theme_saved = true,
            Ok(_) | Err(async_channel::TryRecvError::Empty) => {}
            Err(error) => return Err(error.into()),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not report all command results"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        failures,
        [FailureKind::Paste(424242), FailureKind::Paste(424243)]
    );
    assert!(capture_error, "explicit capture failure was dropped");
    Ok(())
}

fn next_snapshot(events: &async_channel::Receiver<Event>, generation: u64) -> Vec<ClipboardItem> {
    wait_for(
        events,
        Duration::from_secs(5),
        "worker did not respond",
        |event| match event {
            Event::Snapshot {
                items,
                generation: actual,
                ..
            } if actual == generation => Some(items),
            Event::Error(message) => panic!("{message}"),
            Event::CommandFailed { message, .. } | Event::BackgroundError(message) => {
                panic!("{message}")
            }
            _ => None,
        },
    )
}

/// Polls the event stream until `match_event` returns `Some`, discarding
/// unrelated events. Panics with `timeout_message` when nothing matches.
fn wait_for<T>(
    events: &async_channel::Receiver<Event>,
    timeout: Duration,
    timeout_message: &str,
    mut match_event: impl FnMut(Event) -> Option<T>,
) -> T {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(event) = events.try_recv()
            && let Some(value) = match_event(event)
        {
            return value;
        }
        assert!(std::time::Instant::now() < deadline, "{timeout_message}");
        thread::sleep(Duration::from_millis(10));
    }
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
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match events.try_recv() {
            Ok(Event::Error(message)) => {
                assert!(message.contains("读取历史记录失败"), "{message}");
                break;
            }
            Ok(Event::Snapshot { .. }) => bail!("损坏记录被错误地显示为正常快照"),
            _ => {}
        }
        assert!(std::time::Instant::now() < deadline, "启动错误未上报");
        thread::sleep(Duration::from_millis(10));
    }
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 7,
    })?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match events.try_recv() {
            Ok(Event::CommandFailed {
                kind: FailureKind::Query(7),
                message,
            }) => {
                assert!(message.contains("读取历史记录失败"), "{message}");
                break;
            }
            Ok(Event::Snapshot { .. }) => bail!("损坏记录被错误地显示为正常快照"),
            _ => {}
        }
        assert!(
            std::time::Instant::now() < deadline,
            "查询错误未携带请求代次"
        );
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn imported_groups_can_be_browsed_and_reordered_without_mixing_default_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let default_item = history.capture("default item")?.unwrap();
    let first = history.capture("custom first")?.unwrap();
    let second = history.capture("custom second")?.unwrap();
    let groups = GroupRepository::new(&db);
    let group = groups.create("导入分组", None)?;
    groups.move_item_to_group(first, Some(group.id))?;
    groups.move_item_to_group(second, Some(group.id))?;
    drop(history);
    drop(db);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    let Event::Groups(available) = events.recv_blocking()? else {
        bail!("启动时没有发送分组列表");
    };
    assert_eq!(available[0].id, group.id);
    assert_eq!(next_snapshot(&events, 0)[0].id, default_item);
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    let rows = next_snapshot(&events, 1);
    assert_eq!(
        rows.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![second, first]
    );
    service.send(Command::Reorder {
        from: first,
        to: second,
        after: false,
        favorite_only: false,
        group_id: Some(group.id),
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1)[0].id, first);
    let Event::Reordered { result, .. } = events.recv_blocking()? else {
        bail!("没有排序确认");
    };
    result.map_err(anyhow::Error::msg)?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    assert_eq!(next_snapshot(&events, 2)[0].id, default_item);
    Ok(())
}

#[test]
fn create_group_acknowledges_only_persisted_names() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    next_snapshot(&events, 0);
    service.send(Command::CreateGroup("  工作  ".into()))?;
    let Event::Groups(groups) = events.recv_blocking()? else {
        bail!("创建后没有刷新分组列表");
    };
    assert_eq!(groups[0].name, "工作");
    let Event::GroupCreated(Ok(group)) = events.recv_blocking()? else {
        bail!("创建后没有成功确认");
    };
    assert_eq!(group.id, groups[0].id);
    service.send(Command::CreateGroup("工作".into()))?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupCreated(Err(_))
    ));
    Ok(())
}

#[test]
fn group_reorder_acknowledges_saved_order_and_rejects_missing_target() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let first = history.create_group("工作")?.id;
    let second = history.create_group("归档")?.id;
    drop(history);
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    next_snapshot(&events, 0);
    service.send(Command::ReorderGroup {
        from: first,
        to: second,
        after: true,
    })?;
    let Event::Groups(groups) = events.recv_blocking()? else {
        bail!("排序后没有刷新分组列表");
    };
    assert_eq!(
        groups.iter().map(|group| group.id).collect::<Vec<_>>(),
        vec![second, first]
    );
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupReordered { result: Ok(()), .. }
    ));
    service.send(Command::ReorderGroup {
        from: first,
        to: -1,
        after: false,
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupReordered { result: Err(_), .. }
    ));
    assert_eq!(
        History::open(directory.path().join("clipboard.db"))?
            .groups()?
            .iter()
            .map(|group| group.id)
            .collect::<Vec<_>>(),
        vec![second, first]
    );
    Ok(())
}

#[test]
fn rename_group_acknowledges_saved_name_and_rejects_conflict() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let group = history.create_group("工作")?;
    history.create_group("归档")?;
    drop(history);
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    next_snapshot(&events, 0);
    service.send(Command::RenameGroup {
        id: group.id,
        name: "  常用  ".into(),
    })?;
    let Event::Groups(groups) = events.recv_blocking()? else {
        bail!("没有刷新分组列表");
    };
    assert_eq!(groups[0].name, "常用");
    assert!(
        matches!(events.recv_blocking()?, Event::GroupRenamed(Ok(ref renamed)) if renamed.id == group.id && renamed.name == "常用")
    );
    service.send(Command::RenameGroup {
        id: group.id,
        name: "归档".into(),
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupRenamed(Err(_))
    ));
    Ok(())
}

#[test]
fn delete_group_preserves_items_and_rejects_stale_view() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let item = history.capture("keep this")?.unwrap();
    let group = history.create_group("工作")?;
    history.move_to_group(item, None, Some(group.id))?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1)[0].id, item);
    service.send(Command::DeleteGroup {
        id: group.id,
        generation: 0,
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupDeleted(Err(_))
    ));
    service.send(Command::DeleteGroup {
        id: group.id,
        generation: 1,
    })?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(ref groups) if groups.is_empty()));
    assert!(matches!(
        events.recv_blocking()?,
        Event::GroupDeleted(Ok(1))
    ));
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    assert_eq!(next_snapshot(&events, 2)[0].id, item);
    assert_eq!(History::open(path)?.item(item)?.group_id, None);
    Ok(())
}

#[test]
fn text_edit_refreshes_snapshot_and_reports_stale_content() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let id = history.capture("before edit")?.unwrap();
    let hash = history.item(id)?.content_hash;
    drop(history);
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    next_snapshot(&events, 0);
    service.send(Command::EditText {
        id,
        expected_hash: hash.clone(),
        new_text: "after edit".into(),
        generation: 7,
    })?;
    assert_eq!(
        next_snapshot(&events, 0)[0].preview.as_deref(),
        Some("after edit")
    );
    assert!(matches!(
        events.recv_blocking()?,
        Event::TextEdited { id: edited, generation: 7, result: Ok(true) } if edited == id
    ));
    service.send(Command::EditText {
        id,
        expected_hash: hash,
        new_text: "stale overwrite".into(),
        generation: 8,
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::TextEdited { result: Err(_), .. }
    ));
    assert_eq!(History::open(path)?.text(id)?, "after edit");
    Ok(())
}

#[test]
fn move_to_group_updates_visible_history_and_rejects_stale_view() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let id = history.capture("move me")?.unwrap();
    let group = history.create_group("工作")?;
    drop(history);
    drop(db);
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    assert_eq!(next_snapshot(&events, 0)[0].id, id);
    service.send(Command::MoveToGroup {
        id,
        source_group_id: None,
        target_group_id: Some(group.id),
        generation: 0,
    })?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    assert!(next_snapshot(&events, 0).is_empty());
    assert!(
        matches!(events.recv_blocking()?, Event::ItemMoved { id: moved, result: Ok(()) } if moved == id)
    );
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1)[0].id, id);
    service.send(Command::MoveToGroup {
        id,
        source_group_id: Some(group.id),
        target_group_id: None,
        generation: 0,
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::ItemMoved { result: Err(_), .. }
    ));
    assert_eq!(
        History::open(directory.path().join("clipboard.db"))?
            .item(id)?
            .group_id,
        Some(group.id)
    );
    Ok(())
}

#[test]
fn worker_serializes_capture_search_and_delete_without_touching_clipboard() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("中文%_ test".into()))?;
    service.send(Command::Capture("another record".into()))?;
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    let rows = next_snapshot(&events, 1);
    assert_eq!(rows.len(), 1);
    service.send(Command::Delete(rows[0].id))?;
    service.send(Command::Query {
        search: "".into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    assert_eq!(next_snapshot(&events, 2).len(), 1);
    drop(service);
    let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(next_snapshot(&events, 0).len(), 1);
    drop(reopened);
    Ok(())
}

#[test]
fn capture_error_does_not_replace_an_unrelated_settings_acknowledgement() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);

    service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(events.recv_blocking()?, Event::BackgroundError(_)));
    assert!(matches!(
        events.recv_blocking()?,
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn reorder_acknowledges_success_and_rejects_stale_view() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("first".into()))?;
    let first = next_snapshot(&events, 0)[0].id;
    service.send(Command::Capture("second".into()))?;
    let second = next_snapshot(&events, 0)[0].id;
    for generation in [0, 99] {
        service.send(Command::Reorder {
            from: first,
            to: second,
            after: false,
            favorite_only: false,
            group_id: None,
            generation,
        })?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "reorder timed out",
            |event| match event {
                Event::Reordered {
                    from,
                    generation: actual,
                    result,
                } => {
                    assert_eq!((from, actual), (first, generation));
                    assert_eq!(result.is_ok(), generation == 0);
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    service.send(Command::TogglePin(second))?;
    let rows = next_snapshot(&events, 0);
    assert!(rows[0].is_pinned);
    assert_eq!(rows[0].id, second);
    service.send(Command::Reorder {
        from: first,
        to: second,
        after: false,
        favorite_only: false,
        group_id: None,
        generation: 0,
    })?;
    let rows = next_snapshot(&events, 0);
    assert_eq!(
        rows.iter()
            .map(|item| (item.id, item.is_pinned))
            .collect::<Vec<_>>(),
        vec![(first, true), (second, true)]
    );
    assert!(matches!(
        events.recv_blocking()?,
        Event::Reordered { result: Ok(()), .. }
    ));
    drop(service);
    let (_service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    let rows = next_snapshot(&events, 0);
    assert_eq!(rows[0].id, first);
    assert!(rows[0].is_pinned);
    Ok(())
}

#[test]
fn theme_acknowledgement_is_persisted_before_reopening() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_theme, ThemePreference::System);
    next_snapshot(&events, 0);
    for theme in [
        ThemePreference::Light,
        ThemePreference::System,
        ThemePreference::Dark,
    ] {
        service.send(Command::SetTheme(theme))?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "theme timed out",
            |event| match event {
                Event::ThemeSaved(result) => {
                    assert_eq!(result.unwrap(), theme);
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    drop(service);
    let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_theme, ThemePreference::Dark);
    assert!(next_snapshot(&events, 0).is_empty());
    Ok(())
}

#[test]
fn language_defaults_to_chinese_and_is_loaded_after_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_language, LanguagePreference::Chinese);
    next_snapshot(&events, 0);
    service.send(Command::SetLanguage(LanguagePreference::English))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "language save timed out",
        |event| match event {
            Event::LanguageSaved(result) => {
                assert_eq!(result.unwrap(), LanguagePreference::English);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_language, LanguagePreference::English);
    Ok(())
}

#[test]
fn hotkey_change_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_hotkey, HotkeyPreference::CtrlShiftV);
    next_snapshot(&events, 0);
    service.send(Command::SetHotkey(HotkeyPreference::AltC))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hotkey save timed out",
        |event| match event {
            Event::HotkeySaved(result) => {
                assert_eq!(result.unwrap(), HotkeyPreference::AltC);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_hotkey, HotkeyPreference::AltC);
    Ok(())
}

#[test]
fn window_size_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_window_size, None);
    next_snapshot(&events, 0);
    let size = WindowSizePreference::new(960, 720).unwrap();
    service.send(Command::SetWindowSize(size))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "window size save timed out",
        |event| match event {
            Event::WindowSizeSaved(result) => {
                assert_eq!(result.unwrap(), size);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_window_size, Some(size));
    Ok(())
}

#[test]
fn window_position_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        service.initial_window_position,
        WindowPositionPreference::FollowCursor
    );
    next_snapshot(&events, 0);
    service.send(Command::SetWindowPosition(
        WindowPositionPreference::ScreenCenter,
    ))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "window position save timed out",
        |event| match event {
            Event::WindowPositionSaved(result) => {
                assert_eq!(result.unwrap(), WindowPositionPreference::ScreenCenter);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        reopened.initial_window_position,
        WindowPositionPreference::ScreenCenter
    );
    Ok(())
}

#[test]
fn window_behavior_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(service.initial_persist_window_size);
    assert!(!service.initial_auto_reset_state);
    assert!(!service.initial_search_auto_focus);
    assert!(service.initial_search_auto_clear);
    assert!(!service.initial_skip_clear_confirm);
    assert!(service.initial_paste_close_window);
    assert_eq!(service.initial_paste_key, PasteKeyPreference::CtrlV);
    assert!(service.initial_paste_move_to_top);
    assert!(!service.initial_quick_paste_enabled);
    next_snapshot(&events, 0);
    service.send(Command::SetPersistWindowSize(false))?;
    service.send(Command::SetAutoResetState(true))?;
    service.send(Command::SetSearchAutoFocus(true))?;
    service.send(Command::SetSearchAutoClear(false))?;
    service.send(Command::SetSkipClearConfirm(true))?;
    service.send(Command::SetPasteCloseWindow(false))?;
    service.send(Command::SetPasteKey(PasteKeyPreference::ShiftInsert))?;
    service.send(Command::SetPasteMoveToTop(false))?;
    service.send(Command::SetQuickPasteEnabled(true))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut size_acknowledged = false;
    let mut reset_acknowledged = false;
    let mut focus_acknowledged = false;
    let mut clear_acknowledged = false;
    let mut skip_clear_acknowledged = false;
    let mut paste_acknowledged = false;
    let mut paste_key_acknowledged = false;
    let mut move_acknowledged = false;
    let mut quick_acknowledged = false;
    while !size_acknowledged
        || !reset_acknowledged
        || !focus_acknowledged
        || !clear_acknowledged
        || !skip_clear_acknowledged
        || !paste_acknowledged
        || !paste_key_acknowledged
        || !move_acknowledged
        || !quick_acknowledged
    {
        match events.try_recv() {
            Ok(Event::PersistWindowSizeSaved(result)) => {
                assert!(!result.unwrap());
                size_acknowledged = true;
            }
            Ok(Event::AutoResetStateSaved(result)) => {
                assert!(result.unwrap());
                reset_acknowledged = true;
            }
            Ok(Event::SearchAutoFocusSaved(result)) => {
                assert!(result.unwrap());
                focus_acknowledged = true;
            }
            Ok(Event::SearchAutoClearSaved(result)) => {
                assert!(!result.unwrap());
                clear_acknowledged = true;
            }
            Ok(Event::SkipClearConfirmSaved(result)) => {
                assert!(result.unwrap());
                skip_clear_acknowledged = true;
            }
            Ok(Event::PasteCloseWindowSaved(result)) => {
                assert!(!result.unwrap());
                paste_acknowledged = true;
            }
            Ok(Event::PasteKeySaved(result)) => {
                assert_eq!(result.unwrap(), PasteKeyPreference::ShiftInsert);
                paste_key_acknowledged = true;
            }
            Ok(Event::PasteMoveToTopSaved(result)) => {
                assert!(!result.unwrap());
                move_acknowledged = true;
            }
            Ok(Event::QuickPasteEnabledSaved(result)) => {
                assert!(result.unwrap());
                quick_acknowledged = true;
            }
            Ok(Event::Error(message)) => panic!("{message}"),
            _ => {}
        }
        assert!(
            std::time::Instant::now() < deadline,
            "window behavior save timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(!reopened.initial_persist_window_size);
    assert!(reopened.initial_auto_reset_state);
    assert!(reopened.initial_search_auto_focus);
    assert!(!reopened.initial_search_auto_clear);
    assert!(reopened.initial_skip_clear_confirm);
    assert!(!reopened.initial_paste_close_window);
    assert_eq!(reopened.initial_paste_key, PasteKeyPreference::ShiftInsert);
    assert!(!reopened.initial_paste_move_to_top);
    assert!(reopened.initial_quick_paste_enabled);
    Ok(())
}

#[test]
fn paste_shortcuts_save_atomically_and_reject_window_hotkey_conflicts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let mut shortcuts = service.initial_paste_shortcuts.clone();
    shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
    service.send(Command::SetPasteShortcuts(Box::new(shortcuts.clone())))?;
    let result = loop {
        match events.recv_blocking()? {
            Event::PasteShortcutsSaved(result) => break result,
            Event::Error(message) => anyhow::bail!("{message}"),
            _ => {}
        }
    };
    assert_eq!(*result.unwrap(), shortcuts);
    let mut invalid = shortcuts.clone();
    invalid.set_slot(true, 10, "Ctrl+Shift+V".into())?;
    service.send(Command::SetPasteShortcuts(Box::new(invalid)))?;
    let result = loop {
        match events.recv_blocking()? {
            Event::PasteShortcutsSaved(result) => break result,
            Event::Error(message) => anyhow::bail!("{message}"),
            _ => {}
        }
    };
    assert!(result.is_err());
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_paste_shortcuts, shortcuts);
    Ok(())
}

#[test]
fn bump_to_top_refreshes_the_visible_history_without_clipboard_access() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let older = history.capture("older item")?.unwrap();
    let newer = history.capture("newer item")?.unwrap();
    drop(history);
    drop(db);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(next_snapshot(&events, 0)[0].id, newer);
    service.send(Command::BumpToTop(older))?;
    assert_eq!(next_snapshot(&events, 0)[0].id, older);
    Ok(())
}

#[test]
fn quick_paste_resolves_recent_and_favorite_slots_with_group_isolation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let favorite = history.capture("favorite")?.unwrap();
    history.toggle_favorite(favorite)?;
    let recent = history.capture("recent")?.unwrap();
    let grouped = history.capture("grouped")?.unwrap();
    let groups = GroupRepository::new(&db);
    let group = groups.create("Group", None)?;
    groups.move_item_to_group(grouped, Some(group.id))?;
    drop(history);
    drop(db);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    for (slot, is_favorite, group_id, expected) in [
        (1, false, None, Some(recent)),
        (1, true, None, Some(favorite)),
        (1, false, Some(group.id), Some(grouped)),
        (2, false, Some(group.id), None),
        (0, false, None, None),
    ] {
        service.send(Command::ResolveQuickPaste {
            slot,
            favorite: is_favorite,
            group_id,
        })?;
        let Event::QuickPasteResolved {
            slot: actual_slot,
            favorite: actual_favorite,
            result,
        } = events.recv_blocking()?
        else {
            bail!("没有返回快速粘贴槽位结果");
        };
        assert_eq!((actual_slot, actual_favorite), (slot, is_favorite));
        assert_eq!(result.ok(), expected);
    }
    Ok(())
}

#[test]
fn display_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::CardDensity;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_display, DisplayPreference::default());
    next_snapshot(&events, 0);
    let display = DisplayPreference {
        show_category_filter: false,
        show_drag_area_indicator: false,
        card_density: CardDensity::Spacious,
        card_max_lines: 5,
        show_time: false,
        time_format: clipboard_core::preferences::TimeFormat::Relative,
        show_char_count: false,
        show_byte_size: false,
        show_source_app: false,
        source_app_display: clipboard_core::preferences::SourceAppDisplay::Icon,
    };
    service.send(Command::SetDisplay(display))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "display save timed out",
        |event| match event {
            Event::DisplaySaved(result) => {
                assert_eq!(result.unwrap(), display);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_display, display);
    Ok(())
}

#[test]
fn audio_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::SoundTiming;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_audio, AudioPreference::default());
    next_snapshot(&events, 0);
    let audio = AudioPreference {
        copy_enabled: true,
        copy_timing: SoundTiming::AfterSuccess,
        paste_enabled: true,
        paste_timing: SoundTiming::AfterSuccess,
    };
    service.send(Command::SetAudio(audio))?;
    assert!(matches!(events.recv_blocking()?, Event::AudioSaved(Ok(saved)) if saved == audio));
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_audio, audio);
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
                png: include_bytes!("../../tests/fixtures/test.png").to_vec(),
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
fn hover_preview_preferences_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::HoverPreviewPosition;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        service.initial_hover_preview,
        HoverPreviewPreference::default()
    );
    next_snapshot(&events, 0);
    let preference = HoverPreviewPreference {
        text: true,
        expanded_image: true,
        delay_ms: 750,
        position: HoverPreviewPosition::Left,
        zoom_step: 20,
        ..HoverPreviewPreference::default()
    };
    service.send(Command::SetHoverPreview(preference))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hover settings timed out",
        |event| match event {
            Event::HoverPreviewSaved(result) => {
                assert_eq!(result.unwrap(), preference);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_hover_preview, preference);
    Ok(())
}

#[test]
fn worker_keeps_favorite_filter_during_capture_and_toggle() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("favorite %_".into()))?;
    let id = next_snapshot(&events, 0)[0].id;
    service.send(Command::ToggleFavorite(id))?;
    assert!(next_snapshot(&events, 0)[0].is_favorite);
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: true,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    service.send(Command::Capture("ordinary %_".into()))?;
    assert_eq!(next_snapshot(&events, 1)[0].id, id);
    service.send(Command::ToggleFavorite(id))?;
    assert!(next_snapshot(&events, 1).is_empty());
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    assert_eq!(next_snapshot(&events, 2).len(), 2);
    Ok(())
}

#[test]
fn worker_keeps_category_filter_when_history_changes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("first text".into()))?;
    next_snapshot(&events, 0);
    service.send(Command::CaptureImage {
        png: include_bytes!("../../tests/fixtures/test.png").to_vec(),
        width: 128,
        height: 128,
    })?;
    assert_eq!(next_snapshot(&events, 0).len(), 2);
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::Text,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    service.send(Command::Capture("second text".into()))?;
    let rows = next_snapshot(&events, 1);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|item| item.content_type == "text"));
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::Other,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 2,
    })?;
    let rows = next_snapshot(&events, 2);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content_type, "image");
    Ok(())
}

#[test]
fn image_capture_is_persisted_and_delete_removes_managed_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let png = include_bytes!("../../tests/fixtures/test.png");
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
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert!(show_existing_instance(Some(directory.path().to_owned()))?);
    // Keep the receiver stalled while both producers reach the full channel.
    thread::sleep(Duration::from_millis(50));
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
    // Give the instance listener time to encounter the full queue before draining it.
    thread::sleep(Duration::from_millis(50));
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

#[test]
fn pause_acknowledgement_is_ordered_and_does_not_change_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(!service.initial_paused);
    next_snapshot(&events, 0);
    for paused in [true, false, true] {
        service.send(Command::Pause(paused))?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "pause was not acknowledged",
            |event| match event {
                Event::Paused(actual) => {
                    assert_eq!(actual, paused);
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert!(next_snapshot(&events, 1).is_empty());
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(reopened.initial_paused);
    Ok(())
}

#[test]
fn worker_exports_backup_and_cli_restores_into_isolated_directory() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source");
    let restored = directory.path().join("restored");
    let backup = directory.path().join("history.zip");
    let (service, events) = Service::start(Some(source), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Capture("worker backup".into()))?;
    let original = next_snapshot(&events, 0)[0].id;
    service.send(Command::ExportBackup(backup.clone()))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "backup timed out",
        |event| match event {
            Event::BackupExported(result) => {
                let report = result.unwrap_or_else(|error| panic!("backup failed: {error}"));
                assert_eq!(report.total_items, 1);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    let (database, report) = restore_backup_data(&backup, restored)?;
    assert_eq!(report.total_items, 1);
    assert_eq!(History::open(database)?.text(original)?, "worker backup");
    Ok(())
}

#[test]
fn clear_history_preserves_favorites_and_rejects_stale_group_view() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let group = history.create_group("Work")?;
    let removed = history.capture("remove")?.unwrap();
    history.move_to_group(removed, None, Some(group.id))?;
    let favorite = history.capture("favorite")?.unwrap();
    history.move_to_group(favorite, None, Some(group.id))?;
    history.toggle_favorite(favorite)?;
    let default = history.capture("default")?.unwrap();
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: "favorite".into(),
        favorite_only: true,
        category: ContentCategory::All,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    service.send(Command::ClearHistory {
        group_id: Some(group.id),
        generation: 0,
    })?;
    assert!(matches!(
        events.recv_blocking()?,
        Event::HistoryCleared(Err(_))
    ));
    service.send(Command::ClearHistory {
        group_id: Some(group.id),
        generation: 1,
    })?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    assert!(matches!(
        events.recv_blocking()?,
        Event::HistoryCleared(Ok(1))
    ));
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert!(history.item(removed).is_err());
    assert!(history.item(favorite)?.is_favorite);
    assert_eq!(history.text(default)?, "default");
    Ok(())
}

#[test]
fn clear_all_history_removes_protected_items_across_groups() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let group = history.create_group("Keep group")?;
    let pinned = history.capture("pinned")?.unwrap();
    history.toggle_pin(pinned)?;
    let favorite = history.capture("favorite")?.unwrap();
    history.toggle_favorite(favorite)?;
    history.move_to_group(favorite, None, Some(group.id))?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::ClearAllHistory)?;
    assert!(next_snapshot(&events, 0).is_empty());
    assert!(matches!(
        events.recv_blocking()?,
        Event::AllHistoryCleared(Ok(2))
    ));
    drop(service);

    let history = History::open(directory.path().join("clipboard.db"))?;
    assert_eq!(history.count("", false)?, 0);
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 0);
    assert!(history.groups()?.iter().any(|item| item.id == group.id));
    Ok(())
}

#[test]
fn batch_delete_rejects_stale_or_cross_group_selection() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let group = history.create_group("Work")?;
    let first = history.capture("first")?.unwrap();
    let second = history.capture("second")?.unwrap();
    history.move_to_group(first, None, Some(group.id))?;
    history.move_to_group(second, None, Some(group.id))?;
    let other = history.capture("other")?.unwrap();
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 2);
    for (ids, generation) in [(vec![first, second], 0), (vec![first, other], 1)] {
        service.send(Command::DeleteBatch {
            ids,
            group_id: Some(group.id),
            generation,
        })?;
        assert!(matches!(
            events.recv_blocking()?,
            Event::BatchDeleted(Err(_))
        ));
    }
    service.send(Command::DeleteBatch {
        ids: vec![first, second],
        group_id: Some(group.id),
        generation: 1,
    })?;
    assert!(matches!(events.recv_blocking()?, Event::Groups(_)));
    assert!(next_snapshot(&events, 1).is_empty());
    assert!(matches!(
        events.recv_blocking()?,
        Event::BatchDeleted(Ok(2))
    ));
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert!(history.item(first).is_err());
    assert!(history.item(second).is_err());
    assert_eq!(history.text(other)?, "other");
    Ok(())
}
