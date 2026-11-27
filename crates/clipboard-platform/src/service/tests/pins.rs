use super::*;

#[test]
fn favorite_pin_refresh_preserves_the_order_and_badges_of_the_all_return_page() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let older = history.capture("older")?.unwrap();
    let newer = history.capture("newer")?.unwrap();
    history.set_favorite(older, true)?;
    history.set_favorite(newer, true)?;
    drop(history);

    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::TogglePin {
        id: newer,
        favorite_only: false,
    })?;
    assert!(next_snapshot(&events, 0)[0].pinned_in(false));
    service.send(Command::Query {
        search: String::new(),
        favorite_only: true,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        offset: 0,
        generation: 1,
    })?;
    assert!(
        next_snapshot(&events, 1)
            .iter()
            .all(|row| !row.pinned_in(true))
    );
    for pinning in [true, false] {
        service.send(Command::TogglePin {
            id: older,
            favorite_only: true,
        })?;
        let Event::Snapshot {
            items,
            all_page: Some((all, _)),
            generation: 1,
            ..
        } = next_event(&events)
        else {
            bail!("missing All return page after pin change");
        };
        let badges = |rows: &[ClipboardItem], favorites| {
            rows.iter()
                .map(|row| (row.id, row.pinned_in(favorites)))
                .collect::<Vec<_>>()
        };
        assert_eq!(badges(&all, false), [(newer, true), (older, false)]);
        let expected = if pinning {
            [(older, true), (newer, false)]
        } else {
            [(newer, false), (older, false)]
        };
        assert_eq!(badges(&items, true), expected);
    }
    Ok(())
}
