use super::*;

#[test]
fn group_create_and_list() {
    let (_dir, db) = temp_db();
    let repo = GroupRepository::new(&db);
    let group = repo.create("Test Group").unwrap();
    assert_eq!(group.name, "Test Group");
    assert_eq!(group.item_count, 0);

    let groups = repo.list_with_count().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Test Group");
}

#[test]
fn group_rename() {
    let (_dir, db) = temp_db();
    let repo = GroupRepository::new(&db);
    let group = repo.create("Old Name").unwrap();
    repo.rename(group.id, "New Name").unwrap();
    let groups = repo.list_with_count().unwrap();
    assert_eq!(groups[0].name, "New Name");
}

#[test]
fn group_delete() {
    let (_dir, db) = temp_db();
    let repo = GroupRepository::new(&db);
    let group = repo.create("To Delete").unwrap();
    repo.delete(group.id).unwrap();
    assert!(repo.list_with_count().unwrap().is_empty());
}

#[test]
fn group_delete_cascades_items() {
    let (_dir, db) = temp_db();
    let group_repo = GroupRepository::new(&db);
    let group = group_repo.create("Cascade Group").unwrap();

    let clip_repo = ClipboardRepository::new(&db);
    let mut item = make_text_item("grouped_item");
    item.group_id = Some(group.id);
    let id = clip_repo.insert(item).unwrap();

    group_repo.delete(group.id).unwrap();
    assert!(clip_repo.get_by_id(id).unwrap().is_none());
}

#[test]
fn group_move_item() {
    let (_dir, db) = temp_db();
    let group_repo = GroupRepository::new(&db);
    let group = group_repo.create("Move Target").unwrap();

    let clip_repo = ClipboardRepository::new(&db);
    let id = clip_repo.insert(make_text_item("movable")).unwrap();

    group_repo.move_item_to_group(id, Some(group.id)).unwrap();
    assert_eq!(
        clip_repo.get_by_id(id).unwrap().unwrap().group_id,
        Some(group.id)
    );

    group_repo.move_item_to_group(id, None).unwrap();
    assert_eq!(clip_repo.get_by_id(id).unwrap().unwrap().group_id, None);
}

#[test]
fn group_item_count() {
    let (_dir, db) = temp_db();
    let group_repo = GroupRepository::new(&db);
    let group = group_repo.create("Counting").unwrap();

    let clip_repo = ClipboardRepository::new(&db);
    let mut item1 = make_text_item("count_a");
    item1.group_id = Some(group.id);
    let mut item2 = make_text_item("count_b");
    item2.group_id = Some(group.id);
    clip_repo.insert(item1).unwrap();
    clip_repo.insert(item2).unwrap();

    let groups = group_repo.list_with_count().unwrap();
    assert_eq!(groups[0].item_count, 2);
}
