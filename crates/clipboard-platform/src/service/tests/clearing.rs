use super::*;

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
        offset: 0,
        generation: 1,
    })?;
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    service.send(Command::ClearHistory {
        group_id: Some(group.id),
        generation: 0,
    })?;
    assert!(matches!(next_event(&events), Event::HistoryCleared(Err(_))));
    service.send(Command::ClearHistory {
        group_id: Some(group.id),
        generation: 1,
    })?;
    assert!(matches!(next_event(&events), Event::Groups(_)));
    assert_eq!(next_snapshot(&events, 1).len(), 1);
    assert!(
        matches!(next_event(&events), Event::HistoryCleared(Ok(outcome)) if outcome.deleted == 1 && outcome.cleanup_warning.is_none())
    );
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
    history.toggle_pin(pinned, false)?;
    let favorite = history.capture("favorite")?.unwrap();
    history.toggle_favorite(favorite)?;
    history.move_to_group(favorite, None, Some(group.id))?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::ClearAllHistory)?;
    assert!(next_snapshot(&events, 0).is_empty());
    assert!(matches!(
        next_event(&events),
        Event::AllHistoryCleared(Ok(outcome)) if outcome.deleted == 2 && outcome.cleanup_warning.is_none()
    ));
    drop(service);

    let history = History::open(directory.path().join("clipboard.db"))?;
    assert_eq!(history.count("", false)?, 0);
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 0);
    assert!(history.groups()?.iter().any(|item| item.id == group.id));
    Ok(())
}
