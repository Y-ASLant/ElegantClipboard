use super::*;

#[test]
fn media_reference_queries_fail_if_a_path_cannot_be_read() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("media reference")).unwrap();
    db.write_connection()
        .lock()
        .execute(
            "UPDATE clipboard_items SET image_path = X'01', file_payload = X'01' WHERE id = ?1",
            params![id],
        )
        .unwrap();

    assert!(repo.get_all_image_paths().is_err());
    assert!(repo.get_all_file_payloads().is_err());
    assert_eq!(repo.count(QueryOptions::default()).unwrap(), 1);
}

#[test]
fn clear_history_preserves_pinned_and_favorites() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id1 = repo.insert(make_text_item("normal")).unwrap();
    let id2 = repo.insert(make_text_item("pinned")).unwrap();
    let id3 = repo.insert(make_text_item("favorite")).unwrap();
    repo.toggle_pin(id2).unwrap();
    repo.toggle_favorite(id3).unwrap();

    let deleted = repo.clear_history(None, None).unwrap();
    assert_eq!(deleted, 1);
    assert!(repo.get_by_id(id1).unwrap().is_none());
    assert!(repo.get_by_id(id2).unwrap().is_some());
    assert!(repo.get_by_id(id3).unwrap().is_some());
}

#[test]
fn clear_all() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("all1")).unwrap();
    let id2 = repo.insert(make_text_item("all2")).unwrap();
    repo.toggle_pin(id2).unwrap();
    repo.clear_all().unwrap();
    assert_eq!(repo.count(QueryOptions::default()).unwrap(), 0);
}

#[test]
fn enforce_max_count() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    for i in 0..5 {
        repo.insert(make_text_item(&format!("enforce_{i}")))
            .unwrap();
    }
    let (deleted, _, _) = repo.enforce_max_count(3, None).unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(repo.count(QueryOptions::default()).unwrap(), 3);
}

#[test]
fn enforce_max_count_no_op_when_under_limit() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    repo.insert(make_text_item("under_limit")).unwrap();
    let (deleted, _, _) = repo.enforce_max_count(10, None).unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn enforce_max_count_reports_only_media_from_evicted_rows() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let text = repo.insert(make_text_item("old text")).unwrap();
    let image = repo.insert(make_text_item("old image")).unwrap();
    let files = repo.insert(make_text_item("recent files")).unwrap();
    {
        let connection = db.write_connection();
        let conn = connection.lock();
        for (id, timestamp) in [
            (text, "2020-01-01 00:00:00"),
            (image, "2020-01-02 00:00:00"),
            (files, "2020-01-03 00:00:00"),
        ] {
            conn.execute(
                "UPDATE clipboard_items SET created_at = ?1 WHERE id = ?2",
                params![timestamp, id],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE clipboard_items SET image_path = 'evicted.png' WHERE id = ?1",
            [image],
        )
        .unwrap();
        conn.execute(
            "UPDATE clipboard_items SET file_payload = 'retained.json' WHERE id = ?1",
            [files],
        )
        .unwrap();
    }

    let (deleted, images, payloads) = repo.enforce_max_count(1, None).unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(images, vec!["evicted.png"]);
    assert!(payloads.is_empty());
    assert!(repo.get_by_id(text).unwrap().is_none());
    assert!(repo.get_by_id(image).unwrap().is_none());
    assert_eq!(
        repo.get_by_id(files)
            .unwrap()
            .unwrap()
            .file_payload
            .as_deref(),
        Some("retained.json")
    );
}
