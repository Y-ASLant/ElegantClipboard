use super::*;

#[test]
fn insert_and_get_by_id() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("hello")).unwrap();
    let item = repo.get_by_id(id).unwrap().unwrap();
    assert_eq!(item.text_content.as_deref(), Some("hello"));
    assert_eq!(item.content_type, "text");
}

#[test]
fn get_nonexistent_returns_none() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    assert!(repo.get_by_id(999).unwrap().is_none());
}

#[test]
fn insert_increments_sort_order() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id1 = repo.insert(make_text_item("first")).unwrap();
    let id2 = repo.insert(make_text_item("second")).unwrap();
    let item1 = repo.get_by_id(id1).unwrap().unwrap();
    let item2 = repo.get_by_id(id2).unwrap().unwrap();
    assert!(item2.sort_order > item1.sort_order);
}

#[test]
fn touch_by_hash_updates_sort_order() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let item = make_text_item("touchable");
    let hash = item.content_hash.clone();
    let id = repo.insert(item).unwrap();
    let original = repo.get_by_id(id).unwrap().unwrap();

    repo.insert(make_text_item("spacer")).unwrap();
    let touched_id = repo.touch_by_hash(&hash, None).unwrap();
    assert_eq!(touched_id, Some(id));

    let updated = repo.get_by_id(id).unwrap().unwrap();
    assert!(updated.sort_order > original.sort_order);
    assert!(updated.access_count > original.access_count);
}

#[test]
fn touch_nonexistent_returns_none() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    assert!(repo.touch_by_hash("no_such_hash", None).unwrap().is_none());
}

#[test]
fn delete_item() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("to_delete")).unwrap();
    assert!(repo.get_by_id(id).unwrap().is_some());
    repo.delete(id).unwrap();
    assert!(repo.get_by_id(id).unwrap().is_none());
}

#[test]
fn pin_states_are_independent_and_favorite_pin_ends_with_membership() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("pin_test")).unwrap();
    let pins = || {
        let item = repo.get_by_id(id).unwrap().unwrap();
        (item.pinned_in(false), item.pinned_in(true))
    };
    assert_eq!(pins(), (false, false));
    assert!(repo.toggle_pin(id, true).is_err());
    assert!(repo.toggle_pin(id, false).unwrap());
    assert!(repo.set_favorite(id, true).unwrap());
    assert_eq!(pins(), (true, false));
    assert!(repo.toggle_pin(id, true).unwrap());
    assert!(!repo.set_favorite(id, true).unwrap());
    assert_eq!(pins(), (true, true));
    assert!(!repo.toggle_pin(id, false).unwrap());
    assert_eq!(pins(), (false, true));
    assert!(!repo.toggle_pin(id, true).unwrap());
    assert_eq!(pins(), (false, false));

    assert!(repo.toggle_pin(id, false).unwrap());
    assert!(repo.toggle_pin(id, true).unwrap());
    assert!(repo.set_favorite(id, false).unwrap());
    assert_eq!(pins(), (true, false));
    assert!(repo.toggle_favorite(id).unwrap());
    assert_eq!(pins(), (true, false));
}

#[test]
fn toggle_favorite() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("fav_test")).unwrap();
    let fav = repo.toggle_favorite(id).unwrap();
    assert!(fav);
    let item = repo.get_by_id(id).unwrap().unwrap();
    assert!(item.is_favorite);
    assert!(item.favorite_order > 0);

    let unfav = repo.toggle_favorite(id).unwrap();
    assert!(!unfav);
    let item = repo.get_by_id(id).unwrap().unwrap();
    assert!(!item.is_favorite);
    assert_eq!(item.favorite_order, 0);
}

#[test]
fn bump_to_top() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id1 = repo.insert(make_text_item("bump1")).unwrap();
    let id2 = repo.insert(make_text_item("bump2")).unwrap();
    let sort2 = repo.get_by_id(id2).unwrap().unwrap().sort_order;

    repo.bump_to_top(id1).unwrap();
    let new_sort1 = repo.get_by_id(id1).unwrap().unwrap().sort_order;
    assert!(new_sort1 > sort2);
}

#[test]
fn bump_to_top_skips_pinned() {
    let (_dir, db) = temp_db();
    let repo = ClipboardRepository::new(&db);
    let id = repo.insert(make_text_item("pinned_bump")).unwrap();
    repo.toggle_pin(id, false).unwrap();
    let original_sort = repo.get_by_id(id).unwrap().unwrap().sort_order;
    repo.bump_to_top(id).unwrap();
    assert_eq!(
        repo.get_by_id(id).unwrap().unwrap().sort_order,
        original_sort
    );
}
