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
