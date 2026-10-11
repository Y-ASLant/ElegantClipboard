use super::*;
use clipboard_core::database::GroupRepository;

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
        png: include_bytes!("../../../tests/fixtures/test.png").to_vec(),
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
