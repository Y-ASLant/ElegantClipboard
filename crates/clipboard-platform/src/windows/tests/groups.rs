use super::*;
use clipboard_core::database::GroupRepository;

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
