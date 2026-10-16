use super::*;
use crate::{database::Database, preferences::Preferences};

#[test]
fn limits_are_per_group_and_protect_pinned_and_favorite_items() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let group = history.create_group("work")?;
    let default_old = history.capture("default old")?.unwrap();
    let grouped_old = history.capture("group old")?.unwrap();
    history.move_to_group(grouped_old, None, Some(group.id))?;
    let pin = history.capture("pin")?.unwrap();
    let favorite = history.capture("favorite")?.unwrap();
    history.toggle_pin(pin)?;
    history.toggle_favorite(favorite)?;
    let default_new = history.capture("default new")?.unwrap();
    let grouped_new = history.capture("group new")?.unwrap();
    history.move_to_group(grouped_new, None, Some(group.id))?;
    let result = history.save_retention(
        RetentionPolicy {
            days: 0,
            max_count: 1,
        },
        true,
        &dir.path().join("images"),
    )?;
    assert_eq!(result.deleted, 2);
    assert!(history.item(default_old).is_err());
    assert!(history.item(grouped_old).is_err());
    for id in [pin, favorite, default_new, grouped_new] {
        history.item(id)?;
    }
    Ok(())
}

#[test]
fn upgrading_preserves_old_history_until_age_limit_is_confirmed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let id = history.capture("old history")?.unwrap();
    db.write_connection().lock().execute(
        "UPDATE clipboard_items SET created_at = '2000-01-01 00:00:00'",
        [],
    )?;
    let images = dir.path().join("images");
    assert_eq!(history.apply_retention(&images)?.deleted, 0);
    let policy = RetentionPolicy {
        days: 30,
        max_count: 0,
    };
    assert!(history.save_retention(policy, false, &images).is_err());
    history.item(id)?;
    assert_eq!(
        Preferences::new(&db).retention()?,
        RetentionPolicy::default()
    );
    assert_eq!(history.save_retention(policy, true, &images)?.deleted, 1);
    drop(history);
    drop(db);
    let db = Database::new(dir.path().join("clipboard.db"))?;
    assert_eq!(Preferences::new(&db).retention()?, policy);
    Ok(())
}

#[test]
fn deletion_failure_rolls_back_policy_and_keeps_media() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let first = history.capture("first")?.unwrap();
    let second = history.capture("second")?.unwrap();
    let images = dir.path().join("images");
    std::fs::create_dir(&images)?;
    let image = images.join("old.png");
    std::fs::write(&image, b"owned media")?;
    db.write_connection().lock().execute(
        "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
        params![image.to_string_lossy(), first],
    )?;
    db.write_connection().lock().execute_batch("CREATE TRIGGER reject_retention BEFORE DELETE ON clipboard_items BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;")?;
    assert!(
        history
            .save_retention(
                RetentionPolicy {
                    days: 0,
                    max_count: 1
                },
                true,
                &images
            )
            .is_err()
    );
    assert_eq!(
        Preferences::new(&db).retention()?,
        RetentionPolicy::default()
    );
    history.item(first)?;
    history.item(second)?;
    assert!(image.exists());
    Ok(())
}

#[test]
fn upgrade_preserves_custom_groups_until_first_policy_is_confirmed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let group = history.create_group("legacy custom")?;
    db.write_connection().lock().execute(
        "WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<10001)
         INSERT INTO clipboard_items(content_type, content_hash, semantic_hash, text_content, group_id, created_at)
         SELECT 'text', 'legacy-' || n, 'legacy-' || n, 'entry', ?1, '2000-01-01 00:00:00' FROM numbers;",
        [group.id],
    )?;
    let images = dir.path().join("images");
    assert_eq!(history.apply_retention(&images)?.deleted, 0);
    history.capture_with_media("new default group item", &images)?;
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 10001);
    assert!(!Preferences::new(&db).retention_saved()?);
    assert!(
        history
            .save_retention(RetentionPolicy::default(), false, &images)
            .is_err()
    );
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 10001);
    assert!(!Preferences::new(&db).retention_saved()?);
    assert_eq!(
        history
            .save_retention(RetentionPolicy::default(), true, &images)?
            .deleted,
        1
    );
    assert!(Preferences::new(&db).retention_saved()?);
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 10000);
    assert_eq!(history.count("", false)?, 1);
    Ok(())
}

#[test]
fn unlimited_retention_keeps_aged_items_and_does_not_depend_on_page_limit() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db = Database::new(dir.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = dir.path().join("images");
    history.save_retention(
        RetentionPolicy {
            days: 0,
            max_count: 0,
        },
        true,
        &images,
    )?;
    db.write_connection().lock().execute_batch(
        "WITH RECURSIVE numbers(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<10001)
         INSERT INTO clipboard_items(content_type, content_hash, semantic_hash, text_content, created_at)
         SELECT 'text', 'fixture-' || n, 'fixture-' || n, 'entry', '2000-01-01 00:00:00' FROM numbers;",
    )?;
    assert_eq!(history.apply_retention(&images)?.deleted, 0);
    assert_eq!(history.count("", false)?, 10001);
    assert_eq!(history.list("", i64::MAX, false)?.len(), 10000);
    Ok(())
}
