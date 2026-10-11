use super::*;

#[test]
fn list_default_ordering() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("older")).unwrap();
    repo.insert(make_text_item("newer")).unwrap();

    let items = repo.list(QueryOptions::default()).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items[0].sort_order > items[1].sort_order);
}

#[test]
fn list_reports_invalid_row_instead_of_silently_skipping_it() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    let valid_id = repo.insert(make_text_item("valid")).unwrap();
    let invalid_id = repo.insert(make_text_item("invalid")).unwrap();

    db.write_connection()
        .lock()
        .execute(
            "UPDATE clipboard_items SET byte_size = X'01' WHERE id = ?1",
            params![invalid_id],
        )
        .unwrap();

    assert!(repo.list(QueryOptions::default()).is_err());
    assert_eq!(repo.count(QueryOptions::default()).unwrap(), 2);
    assert_eq!(
        repo.get_by_id(valid_id)
            .unwrap()
            .unwrap()
            .text_content
            .as_deref(),
        Some("valid")
    );
}

#[test]
fn list_with_search() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("hello world")).unwrap();
    repo.insert(make_text_item("goodbye world")).unwrap();
    repo.insert(make_text_item("hello rust")).unwrap();

    let items = repo
        .list(QueryOptions {
            search: Some("hello".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn search_reuses_escaped_pattern_with_filters_and_pagination() {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::new(directory.path().join("clipboard.db")).unwrap();
    let repo = ClipboardRepository::new(&db);
    let group_id = GroupRepository::new(&db).create("search", None).unwrap().id;
    let mut text = make_text_item(r"中文%_\text");
    text.group_id = Some(group_id);
    repo.insert(text).unwrap();
    repo.insert(NewClipboardItem {
        content_type: ContentType::Files,
        file_paths: Some(vec![r"C:\中文%_\file".into()]),
        content_hash: "search-files".into(),
        semantic_hash: "search-files".into(),
        group_id: Some(group_id),
        ..Default::default()
    })
    .unwrap();
    repo.insert(make_text_item(r"中文%_\other group")).unwrap();
    let mut nonmatch = make_text_item("中文ab");
    nonmatch.group_id = Some(group_id);
    repo.insert(nonmatch).unwrap();

    for search in ["%", "_", "\\", "中文%_"] {
        let options = QueryOptions {
            search: Some(search.into()),
            content_type: Some("text,files".into()),
            group_id: Some(group_id),
            limit: Some(1),
            offset: Some(0),
            ..Default::default()
        };
        assert_eq!(repo.count(options.clone()).unwrap(), 2);
        let first = repo.list(options.clone()).unwrap();
        let second = repo
            .list(QueryOptions {
                offset: Some(1),
                ..options
            })
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].id, second[0].id);
    }
}

#[test]
fn list_with_content_type_filter() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("text item")).unwrap();

    let img_item = NewClipboardItem {
        content_type: ContentType::Image,
        content_hash: "img_hash".to_string(),
        semantic_hash: "img_hash".to_string(),
        image_path: Some("/fake/path.png".to_string()),
        ..Default::default()
    };
    repo.insert(img_item).unwrap();

    let text_only = repo
        .list(QueryOptions {
            content_type: Some("text".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(text_only.len(), 1);
}

#[test]
fn list_with_limit_and_offset() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    for i in 0..5 {
        repo.insert(make_text_item(&format!("item_{i}"))).unwrap();
    }

    let items = repo
        .list(QueryOptions {
            limit: Some(2),
            offset: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn count_items() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    for i in 0..3 {
        repo.insert(make_text_item(&format!("count_{i}"))).unwrap();
    }
    let count = repo.count(QueryOptions::default()).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn count_with_filter() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("fav count")).unwrap();
    let id2 = repo.insert(make_text_item("fav count 2")).unwrap();
    repo.toggle_favorite(id2).unwrap();

    let fav_count = repo
        .count(QueryOptions {
            favorite_only: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fav_count, 1);
}

#[test]
fn daily_counts_groups_all_items_from_inclusive_start_date() {
    let db = temp_db();
    let repo = ClipboardRepository::new(&db);
    let group = GroupRepository::new(&db)
        .create("Daily counts group", None)
        .unwrap();

    let entries = [
        ("before boundary", "2026-09-19 23:59:59", false),
        ("at boundary", "2026-09-20 00:00:00", false),
        ("same day grouped", "2026-09-20 12:34:56", true),
        ("same day ungrouped", "2026-09-20 23:59:59", false),
        ("later grouped", "2026-09-22 00:00:00", true),
        ("later deleted", "2026-09-22 16:00:00", false),
        ("latest", "2026-09-23 23:59:59", false),
    ];
    let mut deleted_id = None;
    for (label, timestamp, grouped) in entries {
        let mut item = make_text_item(label);
        if grouped {
            item.group_id = Some(group.id);
        }
        let id = repo.insert(item).unwrap();
        db.write_connection()
            .lock()
            .execute(
                "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
                params![timestamp, id],
            )
            .unwrap();
        if label == "later deleted" {
            deleted_id = Some(id);
        }
    }
    repo.delete(deleted_id.unwrap()).unwrap();

    assert_eq!(
        repo.daily_counts("2026-09-20").unwrap(),
        vec![
            ("2026-09-20".to_string(), 3),
            ("2026-09-22".to_string(), 1),
            ("2026-09-23".to_string(), 1),
        ]
    );
    assert_eq!(
        repo.daily_counts("2026-09-22").unwrap(),
        vec![("2026-09-22".to_string(), 1), ("2026-09-23".to_string(), 1),]
    );
    assert!(repo.daily_counts("2026-09-24").unwrap().is_empty());
}

#[test]
fn get_by_position() {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::new(directory.path().join("positions.db")).unwrap();
    let repo = ClipboardRepository::new(&db);
    let pinned = repo.insert(make_text_item("pinned")).unwrap();
    let favorite = repo.insert(make_text_item("favorite")).unwrap();
    let recent = repo.insert(make_text_item("recent")).unwrap();
    repo.toggle_pin(pinned).unwrap();
    repo.toggle_favorite(pinned).unwrap();
    repo.toggle_favorite(favorite).unwrap();
    let group = GroupRepository::new(&db).create("positions", None).unwrap();
    let grouped_old = repo
        .insert(NewClipboardItem {
            group_id: Some(group.id),
            ..make_text_item("grouped old")
        })
        .unwrap();
    let grouped_recent = repo
        .insert(NewClipboardItem {
            group_id: Some(group.id),
            ..make_text_item("grouped recent")
        })
        .unwrap();
    repo.toggle_favorite(grouped_recent).unwrap();
    repo.toggle_favorite(grouped_old).unwrap();

    for (group_id, expected_recent, expected_favorites) in [
        (None, vec![pinned, recent, favorite], vec![pinned, favorite]),
        (
            Some(group.id),
            vec![grouped_recent, grouped_old],
            vec![grouped_old, grouped_recent],
        ),
    ] {
        for (favorite_only, expected) in [(false, expected_recent), (true, expected_favorites)] {
            for index in 0..=expected.len() {
                let expected_id = expected.get(index).copied();
                let item = if favorite_only {
                    repo.get_favorite_by_position(index, group_id)
                } else {
                    repo.get_by_position(index, group_id)
                }
                .unwrap();
                let id = if favorite_only {
                    repo.get_favorite_id_by_position(index, group_id)
                } else {
                    repo.get_id_by_position(index, group_id)
                }
                .unwrap();
                assert_eq!(item.map(|item| item.id), expected_id);
                assert_eq!(id, expected_id);
            }
        }
    }

    // Slot resolution must not decode body fields that the copy stage will read later.
    db.write_connection()
        .lock()
        .execute(
            "UPDATE clipboard_items SET html_content = x'ff' WHERE id = ?1",
            params![pinned],
        )
        .unwrap();
    assert_eq!(repo.get_id_by_position(0, None).unwrap(), Some(pinned));
    assert_eq!(
        repo.get_favorite_id_by_position(0, None).unwrap(),
        Some(pinned)
    );
    assert!(repo.get_by_id(pinned).is_err());
    assert!(repo.get_by_position(0, None).is_err());
    assert!(repo.get_favorite_by_position(0, None).is_err());
}
