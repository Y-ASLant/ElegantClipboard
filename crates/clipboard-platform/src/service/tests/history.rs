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
    assert!(matches!(next_event(&events), Event::Groups(_)));
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
        next_event(&events),
        Event::TextEdited { id: edited, generation: 7, result: Ok(true) } if edited == id
    ));
    service.send(Command::EditText {
        id,
        expected_hash: hash,
        new_text: "stale overwrite".into(),
        generation: 8,
    })?;
    assert!(matches!(
        next_event(&events),
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
        offset: 0,
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
        offset: 0,
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
    service.send(Command::TogglePin {
        id: second,
        favorite_only: false,
    })?;
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
        next_event(&events),
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
    let group = groups.create("Group")?;
    groups.move_item_to_group(grouped, Some(group.id))?;
    drop(history);
    drop(db);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    for (request_id, (slot, is_favorite, group_id, expected)) in [
        (1, false, None, Some(recent)),
        (1, true, None, Some(favorite)),
        (1, false, Some(group.id), Some(grouped)),
        (2, false, Some(group.id), None),
        (0, false, None, None),
    ]
    .into_iter()
    .enumerate()
    {
        let request_id = request_id as u64;
        service.send(Command::ResolveQuickPaste {
            request_id,
            slot,
            favorite: is_favorite,
            group_id,
        })?;
        let Event::QuickPasteResolved {
            request_id: actual_request_id,
            result,
        } = next_event(&events)
        else {
            bail!("没有返回快速粘贴槽位结果");
        };
        assert_eq!(actual_request_id, request_id);
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
        offset: 0,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    service.send(Command::Capture("ordinary %_".into()))?;
    let Event::Snapshot {
        items,
        all_page: Some((all, total)),
        generation: 1,
        ..
    } = next_event(&events)
    else {
        bail!("收藏中的采集必须同时更新返回全部时使用的首页");
    };
    assert_eq!(items[0].id, id);
    assert_eq!(total, 2);
    assert_eq!(all[0].preview.as_deref(), Some("ordinary %_"));
    service.send(Command::ToggleFavorite(id))?;
    assert!(next_snapshot(&events, 1).is_empty());
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        offset: 0,
        generation: 2,
    })?;
    assert_eq!(next_snapshot(&events, 2).len(), 2);
    Ok(())
}

#[test]
fn worker_set_favorite_add_is_idempotent_and_preserves_history_order() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let previous = history.capture("favorite %_ outside group")?.unwrap();
    history.toggle_favorite(previous)?;
    let previous_order = history.item(previous)?.favorite_order;
    let group = history.create_group("Drop target fixture")?;
    let id = history.capture("favorite %_ in group")?.unwrap();
    history.move_to_group(id, None, Some(group.id))?;
    history.toggle_pin(id, false)?;
    let before = history.item(id)?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: true,
        category: ContentCategory::Text,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        offset: 0,
        generation: 1,
    })?;
    assert!(next_snapshot(&events, 1).is_empty());

    service.send(Command::SetFavorite { id, favorite: true })?;
    let added = next_snapshot(&events, 1);
    let [added] = added.as_slice() else {
        bail!("添加收藏后未返回唯一的匹配记录");
    };
    assert_eq!(added.id, id);
    assert!(added.is_favorite);
    assert_eq!(added.favorite_order, previous_order + 1);
    assert_eq!(added.sort_order, before.sort_order);
    assert_eq!(added.group_id, before.group_id);
    assert_eq!(added.is_pinned, before.is_pinned);

    service.send(Command::SetFavorite { id, favorite: true })?;
    service.send(Command::Query {
        search: "%_".into(),
        favorite_only: true,
        category: ContentCategory::Text,
        group_id: Some(group.id),
        limit: PAGE_SIZE,
        offset: 0,
        generation: 2,
    })?;
    let Event::Snapshot {
        items,
        generation: 2,
        ..
    } = next_event(&events)
    else {
        bail!("重复添加收藏不应发出反馈或刷新，下一事件应来自主动查询");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id);
    assert!(items[0].is_favorite);
    assert_eq!(items[0].favorite_order, added.favorite_order);

    service.send(Command::SetFavorite {
        id: i64::MAX,
        favorite: true,
    })?;
    assert!(matches!(next_event(&events), Event::CommandFailed { .. }));
    Ok(())
}

#[test]
fn worker_set_favorite_remove_preserves_the_pinned_record() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("pinned favorite text")?.unwrap();
    history.toggle_pin(id, false)?;
    history.toggle_favorite(id)?;
    let before = history.item(id)?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: String::new(),
        favorite_only: true,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        offset: 0,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1)[0].id, id);

    service.send(Command::SetFavorite {
        id,
        favorite: false,
    })?;
    assert!(next_snapshot(&events, 1).is_empty());
    service.send(Command::SetFavorite {
        id,
        favorite: false,
    })?;
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        offset: 0,
        generation: 2,
    })?;
    let Event::Snapshot {
        items,
        generation: 2,
        ..
    } = next_event(&events)
    else {
        bail!("重复取消收藏不应发出反馈或刷新，下一事件应来自主动查询");
    };
    let [item] = items.as_slice() else {
        bail!("取消收藏后记录应仍在普通列表");
    };
    assert_eq!(
        (
            item.id,
            item.is_favorite,
            item.favorite_order,
            item.is_pinned,
            item.sort_order,
        ),
        (id, false, 0, true, before.sort_order),
    );
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
        offset: 0,
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
        offset: 0,
        generation: 2,
    })?;
    let rows = next_snapshot(&events, 2);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content_type, "image");
    Ok(())
}

#[test]
fn committed_deletion_reports_id_despite_refresh_or_media_cleanup_failure() -> Result<()> {
    for (operation, fail_refresh, fail_cleanup) in (0..4).flat_map(|operation| {
        [(false, false), (false, true), (true, false), (true, true)]
            .map(|(refresh, cleanup)| (operation, refresh, cleanup))
    }) {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("clipboard.db");
        let history = History::open(database.clone())?;
        let images = directory.path().join("images");
        let id = history.capture_image(b"\x89PNG\r\n\x1a\nsynthetic", 1, 1, &images)?;
        let image = std::path::PathBuf::from(history.item(id)?.image_path.unwrap());
        if fail_cleanup {
            std::fs::remove_file(&image)?;
            std::fs::create_dir(&image)?;
        }
        drop(history);
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        if fail_refresh {
            let db = Database::new(database.clone())?;
            db.write_connection().lock().execute_batch(
                "CREATE TRIGGER break_refresh AFTER DELETE ON clipboard_items BEGIN
                 INSERT INTO clipboard_items (content_type, content_hash, semantic_hash, preview)
                 VALUES ('text', 'refresh-fault', 'refresh-fault', X'01'); END;",
            )?;
        }
        service.send(match operation {
            0 => Command::Delete(id),
            1 => Command::DeleteBatch {
                ids: vec![id],
                group_id: None,
                generation: 0,
            },
            2 => Command::ClearHistory {
                group_id: None,
                generation: 0,
            },
            _ => Command::ClearAllHistory,
        })?;
        if fail_refresh {
            if operation != 0 {
                assert!(matches!(next_event(&events), Event::Groups(_)));
            }
        } else {
            assert!(next_snapshot(&events, 0).is_empty());
        }
        let outcome = match next_event(&events) {
            Event::ItemDeleted {
                id: deleted,
                outcome,
            } if operation == 0 => {
                assert_eq!(deleted, id);
                outcome
            }
            Event::BatchDeleted(Ok(outcome)) if operation == 1 => outcome,
            Event::HistoryCleared(Ok(outcome)) if operation == 2 => outcome,
            Event::AllHistoryCleared(Ok(outcome)) if operation == 3 => outcome,
            _ => panic!("expected committed deletion outcome"),
        };
        assert_eq!(outcome.deleted, 1);
        if operation == 3 {
            // ClearAll 重置 images/icons/staged/blobs，目录障碍不会变成 cleanup_warning。
            assert!(outcome.cleanup_warning.is_none());
            assert!(!image.exists());
        } else {
            assert_eq!(outcome.cleanup_warning.is_some(), fail_cleanup);
            assert_eq!(image.exists(), fail_cleanup);
        }
        if fail_refresh {
            assert!(matches!(next_event(&events), Event::BackgroundError(_)));
        }
        drop(service);
        assert!(History::open(database)?.item(id).is_err());
    }
    Ok(())
}

#[test]
fn rejected_single_delete_reports_failure_without_committed_event() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("clipboard.db");
    let history = History::open(database.clone())?;
    let id = history.capture("keep on failure")?.unwrap();
    let db = Database::new(database)?;
    db.write_connection().lock().execute_batch(
        "CREATE TRIGGER reject_delete BEFORE DELETE ON clipboard_items BEGIN
         SELECT RAISE(ABORT, 'delete denied'); END;",
    )?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Delete(id))?;
    assert!(matches!(next_event(&events), Event::CommandFailed { .. }));
    service.send(Command::QueryDataSize)?;
    assert!(matches!(next_event(&events), Event::DataSize(Ok(_))));
    assert_eq!(history.text(id)?, "keep on failure");
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
        offset: 0,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 2);
    for (ids, generation) in [(vec![first, second], 0), (vec![first, other], 1)] {
        service.send(Command::DeleteBatch {
            ids,
            group_id: Some(group.id),
            generation,
        })?;
        assert!(matches!(next_event(&events), Event::BatchDeleted(Err(_))));
    }
    service.send(Command::DeleteBatch {
        ids: vec![first, second],
        group_id: Some(group.id),
        generation: 1,
    })?;
    assert!(matches!(next_event(&events), Event::Groups(_)));
    assert!(next_snapshot(&events, 1).is_empty());
    assert!(
        matches!(next_event(&events), Event::BatchDeleted(Ok(outcome)) if outcome.deleted == 2 && outcome.cleanup_warning.is_none())
    );
    let history = History::open(directory.path().join("clipboard.db"))?;
    assert!(history.item(first).is_err());
    assert!(history.item(second).is_err());
    assert_eq!(history.text(other)?, "other");
    Ok(())
}

#[test]
fn paged_query_returns_the_requested_window_in_list_order() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let total = clipboard_core::HISTORY_PAGE_ROWS + 20;
    for index in 0..total {
        history.capture(&format!("row {index}"))?;
    }
    drop(history);
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    let startup = next_snapshot(&events, 0);
    assert_eq!(startup.len(), clipboard_core::HISTORY_PAGE_ROWS);
    let query = |offset, generation| Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: clipboard_core::HISTORY_PAGE_ROWS as i64,
        offset,
        generation,
    };
    service.send(query(0, 1))?;
    let first = next_snapshot(&events, 1);
    assert_eq!(first.len(), startup.len());
    assert_eq!(first[0].id, startup[0].id);
    service.send(query(clipboard_core::HISTORY_PAGE_ROWS as i64, 2))?;
    let second = next_snapshot(&events, 2);
    assert_eq!(second.len(), 20);
    assert_eq!(second[0].preview.as_deref(), Some("row 19"));
    assert_eq!(second.last().unwrap().preview.as_deref(), Some("row 0"));
    // Adjacent windows share no row, so paging covers the list exactly once.
    let ids: std::collections::HashSet<_> = first.iter().map(|item| item.id).collect();
    assert!(second.iter().all(|item| !ids.contains(&item.id)));
    Ok(())
}
