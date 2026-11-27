use super::*;
use crate::database::Database;
use rusqlite::params;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nsynthetic";

#[test]
fn image_is_saved_deduplicated_and_missing_file_is_repaired() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = directory.path().join("images");
    let id = history.capture_image(PNG, 3, 2, &images)?;
    let item = history.repo.get_by_id(id)?.unwrap();
    let image_path = std::path::PathBuf::from(item.image_path.unwrap());
    assert_eq!(std::fs::read(&image_path)?, PNG);
    assert_eq!(item.image_width, Some(3));
    assert_eq!(item.image_height, Some(2));
    assert_eq!(history.capture_image(PNG, 3, 2, &images)?, id);
    assert_eq!(history.repo.count(Default::default())?, 1);
    std::fs::remove_file(&image_path)?;
    assert!(history.preview_content(id).is_err());
    assert_eq!(history.capture_image(PNG, 3, 2, &images)?, id);
    assert_eq!(std::fs::read(&image_path)?, PNG);
    assert_eq!(
        history.preview_content(id)?,
        crate::PreviewContent::Image(image_path)
    );
    Ok(())
}

#[test]
fn rejects_oversize_and_invalid_image_without_writing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = directory.path().join("images");
    assert!(history.capture_image(b"not-png", 1, 1, &images).is_err());
    assert!(history.capture_image(PNG, 0, 1, &images).is_err());
    assert!(history.capture_image(PNG, 5001, 5000, &images).is_err());
    assert!(
        history
            .capture_image(&vec![0; MAX_IMAGE_BYTES + 1], 1, 1, &images)
            .is_err()
    );
    assert!(!images.exists());
    Ok(())
}

#[test]
fn delete_removes_only_unreferenced_managed_images() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = directory.path().join("images");
    let id = history.capture_image(PNG, 3, 2, &images)?;
    let managed = PathBuf::from(history.repo.get_by_id(id)?.unwrap().image_path.unwrap());
    let external = directory.path().join("legacy.png");
    std::fs::write(&external, PNG)?;
    let legacy_id = history.repo.insert(NewClipboardItem {
        content_type: ContentType::Image,
        image_path: Some(external.to_str().unwrap().into()),
        content_hash: "legacy-image".into(),
        semantic_hash: "legacy-image".into(),
        ..Default::default()
    })?;
    history.delete_with_media(legacy_id, &images)?;
    assert!(external.exists());
    history.delete_with_media(id, &images)?;
    assert!(!managed.exists());
    Ok(())
}

#[test]
fn clear_history_is_group_scoped_and_preserves_protected_items() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let group = history.create_group("工作")?;
    let group_image = history.capture_image(PNG, 3, 2, &images)?;
    let group_image_path = PathBuf::from(history.item(group_image)?.image_path.unwrap());
    history.move_to_group(group_image, None, Some(group.id))?;
    let group_text = history.capture("remove in group")?.unwrap();
    history.move_to_group(group_text, None, Some(group.id))?;
    let favorite = history.capture("keep favorite")?.unwrap();
    history.move_to_group(favorite, None, Some(group.id))?;
    history.toggle_favorite(favorite)?;
    let pinned = history.capture("keep pinned")?.unwrap();
    history.move_to_group(pinned, None, Some(group.id))?;
    history.toggle_pin(pinned, false)?;
    let default_text = history.capture("keep default")?.unwrap();

    assert_eq!(
        history
            .clear_history_with_media(Some(group.id), &images)?
            .deleted,
        2
    );
    assert!(history.item(group_image).is_err());
    assert!(history.item(group_text).is_err());
    assert!(!group_image_path.exists());
    assert!(history.item(favorite)?.is_favorite);
    assert!(history.item(pinned)?.is_pinned);
    assert_eq!(history.text(default_text)?, "keep default");
    assert_eq!(history.clear_history_with_media(None, &images)?.deleted, 1);
    assert!(history.item(default_text).is_err());
    assert_eq!(history.groups()?[0].item_count, 2);
    Ok(())
}

#[test]
fn clear_all_removes_protected_records_and_media_but_keeps_groups_and_settings() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&database);
    let images = directory.path().join("images");
    let staged_dir = directory.path().join("staged");
    std::fs::create_dir_all(&staged_dir)?;

    let group = history.create_group("Keep group")?;
    let protected = history.capture("protected")?.unwrap();
    history.toggle_pin(protected, false)?;
    history.toggle_favorite(protected)?;
    let grouped = history.capture("grouped")?.unwrap();
    history.move_to_group(grouped, None, Some(group.id))?;
    let image = history.capture_image(PNG, 2, 2, &images)?;
    let image_path = PathBuf::from(history.item(image)?.image_path.unwrap());
    let missing = "Z:\\ElegantClipboard-QA-missing\\clear-all.txt".to_owned();
    let file = history.capture_files(std::slice::from_ref(&missing), &images)?;
    let staged = staged_dir.join("clear-all.txt");
    std::fs::write(&staged, b"staged")?;
    let payload = serde_json::json!({
        "staged": [{"original": missing, "staged": staged.to_string_lossy()}]
    });
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
        params![payload.to_string(), file],
    )?;
    crate::preferences::Preferences::new(&database)
        .set_theme(crate::preferences::ThemePreference::Dark)?;

    let icons = directory.path().join("icons");
    std::fs::create_dir_all(&icons)?;
    let orphan_image = images.join("orphan-leftover.png");
    let orphan_icon = icons.join("orphan-icon.ico");
    std::fs::write(&orphan_image, b"orphan image")?;
    std::fs::write(&orphan_icon, b"orphan icon")?;

    assert_eq!(history.clear_all_with_media(&images)?.deleted, 4);

    assert_eq!(history.count("", false)?, 0);
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 0);
    assert!(history.groups()?.iter().any(|item| item.id == group.id));
    assert_eq!(
        crate::preferences::Preferences::new(&database).theme()?,
        crate::preferences::ThemePreference::Dark
    );
    assert!(!image_path.exists());
    assert!(!staged.exists());
    assert!(!orphan_image.exists());
    assert!(!orphan_icon.exists());
    assert!(images.is_dir());
    assert!(icons.is_dir());
    Ok(())
}

#[test]
fn clear_all_vacuums_database_file() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db_path = directory.path().join("clipboard.db");
    let history = History::open(db_path.clone())?;
    let images = directory.path().join("images");
    // 保持读写双连接占用（贴近 Service 运行时），再塞大块空闲页。
    let _read_hold = history.db.read_connection();
    let _ = _read_hold
        .lock()
        .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))?;
    {
        let connection = history.db.write_connection();
        let connection = connection.lock();
        connection.execute_batch(
            "CREATE TABLE clear_all_fixture (payload BLOB NOT NULL);
             INSERT INTO clear_all_fixture(payload) VALUES (zeroblob(8 * 1024 * 1024));
             DELETE FROM clear_all_fixture;
             DROP TABLE clear_all_fixture;
             PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
    }
    let before = std::fs::metadata(&db_path)?.len();
    let outcome = history.clear_all_with_media(&images)?;
    assert!(
        outcome.cleanup_warning.is_none(),
        "cleanup/vacuum warning: {:?}",
        outcome.cleanup_warning
    );
    let after = std::fs::metadata(&db_path)?.len();
    assert!(
        after < before / 2,
        "expected vacuum shrink before={before} after={after}"
    );
    // 只读连接应已挂回，仍可查询。
    assert_eq!(history.count("", false)?, 0);
    Ok(())
}

#[test]
fn clear_history_stops_before_deleting_rows_with_unreadable_media_paths() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = directory.path().join("images");
    let id = history.capture_image(PNG, 3, 2, &images)?;
    let path = PathBuf::from(history.item(id)?.image_path.unwrap());
    db.write_connection().lock().execute(
        "UPDATE clipboard_items SET image_path = X'01' WHERE id = ?1",
        params![id],
    )?;

    assert!(history.clear_history_with_media(None, &images).is_err());
    assert_eq!(history.repo.count(Default::default())?, 1);
    assert!(path.exists());
    Ok(())
}

#[test]
fn batch_delete_is_atomic_and_cleans_only_selected_media() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let group = history.create_group("批量测试")?;
    let image = history.capture_image(PNG, 3, 2, &images)?;
    let image_path = PathBuf::from(history.item(image)?.image_path.unwrap());
    history.move_to_group(image, None, Some(group.id))?;
    history.toggle_pin(image, false)?;

    let staged_dir = directory.path().join("staged");
    std::fs::create_dir_all(&staged_dir)?;
    let staged = staged_dir.join("draft.txt");
    std::fs::write(&staged, b"draft")?;
    let original = "Z:\\missing\\draft.txt".to_owned();
    let file = history.capture_files(std::slice::from_ref(&original), &images)?;
    history.move_to_group(file, None, Some(group.id))?;
    history.toggle_favorite(file)?;
    let payload = serde_json::json!({
        "staged": [{"original": original, "staged": staged.to_string_lossy()}]
    });
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
        params![payload.to_string(), file],
    )?;
    let other = history.capture("keep in default")?.unwrap();

    assert!(
        history
            .delete_batch_with_media(&[image, other], Some(group.id), &images)
            .is_err()
    );
    assert!(
        history
            .delete_batch_with_media(&[image, image], Some(group.id), &images)
            .is_err()
    );
    history
        .db
        .write_connection()
        .lock()
        .execute_batch(&format!(
            "CREATE TRIGGER reject_batch BEFORE DELETE ON clipboard_items
             WHEN OLD.id = {file} BEGIN SELECT RAISE(ABORT, 'test failure'); END;"
        ))?;
    assert!(
        history
            .delete_batch_with_media(&[image, file], Some(group.id), &images)
            .is_err()
    );
    assert!(history.item(image).is_ok());
    assert!(history.item(file).is_ok());
    assert!(image_path.exists());
    assert!(staged.exists());
    history
        .db
        .write_connection()
        .lock()
        .execute_batch("DROP TRIGGER reject_batch")?;

    assert_eq!(
        history
            .delete_batch_with_media(&[image, file], Some(group.id), &images)?
            .deleted,
        2
    );
    assert!(history.item(image).is_err());
    assert!(history.item(file).is_err());
    assert!(!image_path.exists());
    assert!(!staged.exists());
    assert_eq!(history.text(other)?, "keep in default");
    Ok(())
}

#[test]
fn committed_deletions_report_cleanup_failures_and_continue_other_media() -> Result<()> {
    for operation in 0..4 {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let id = history.capture_image(PNG, 3, 2, &images)?;
        let image = PathBuf::from(history.item(id)?.image_path.unwrap());
        std::fs::remove_file(&image)?;
        std::fs::create_dir(&image)?;
        let staged = directory.path().join("staged");
        std::fs::create_dir(&staged)?;
        let blocked = staged.join("blocked");
        std::fs::create_dir(&blocked)?;
        let removable = staged.join("removable.txt");
        std::fs::write(&removable, b"remove")?;
        let payload = serde_json::json!({"staged": [
            {"staged": blocked}, {"staged": removable}
        ]});
        history.db.write_connection().lock().execute(
            "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
            params![payload.to_string(), id],
        )?;
        let outcome = match operation {
            0 => history.delete_with_media(id, &images)?,
            1 => history.delete_batch_with_media(&[id], None, &images)?,
            2 => history.clear_history_with_media(None, &images)?,
            _ => history.clear_all_with_media(&images)?,
        };
        assert_eq!(outcome.deleted, 1);
        assert!(history.item(id).is_err());
        if operation == 3 {
            // clear_all 整目录重置，不走单文件回收告警。
            assert!(outcome.cleanup_warning.is_none());
            assert!(!image.exists());
            assert!(!blocked.exists());
            assert!(!removable.exists());
            assert!(images.is_dir());
            assert!(staged.is_dir());
        } else {
            assert!(outcome.cleanup_warning.is_some());
            assert!(image.is_dir());
            assert!(blocked.is_dir());
            assert!(!removable.exists());
        }
    }
    Ok(())
}

#[test]
fn clear_ops_roll_back_native_formats_when_item_delete_fails() -> Result<()> {
    use crate::NativeFormatCapture;

    for clear_all in [false, true] {
        let directory = tempfile::tempdir()?;
        let db = Database::new(directory.path().join("clipboard.db"))?;
        let history = History::new(&db);
        let images = directory.path().join("images");
        let id = history.capture("keep-me")?.unwrap();
        history.attach_native_formats(
            id,
            vec![NativeFormatCapture {
                name: "Example.Custom".into(),
                data: b"payload".to_vec(),
            }],
        )?;
        assert_eq!(history.native_formats_for_copy(id)?.len(), 1);
        db.write_connection().lock().execute_batch(
            "CREATE TRIGGER fail_del BEFORE DELETE ON clipboard_items BEGIN
               SELECT RAISE(ABORT, 'simulated delete failure');
             END;",
        )?;
        let result = if clear_all {
            history.clear_all_with_media(&images)
        } else {
            history.clear_history_with_media(None, &images)
        };
        assert!(result.is_err(), "clear_all={clear_all}");
        assert!(history.item(id).is_ok(), "主记录应回滚保留");
        assert_eq!(
            history.native_formats_for_copy(id)?.len(),
            1,
            "原生格式应与主记录同事务回滚 clear_all={clear_all}"
        );
    }
    Ok(())
}

#[test]
fn missing_media_is_already_cleaned_and_shared_payload_errors_prevent_deletion() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let id = history.capture_image(PNG, 3, 2, &images)?;
    std::fs::remove_file(history.item(id)?.image_path.unwrap())?;
    assert!(
        history
            .delete_with_media(id, &images)?
            .cleanup_warning
            .is_none()
    );
    let staged = directory.path().join("staged");
    std::fs::create_dir(&staged)?;
    let shared = staged.join("shared.txt");
    std::fs::write(&shared, b"shared")?;
    let preserved = history.capture("preserved")?.unwrap();
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = '{broken' WHERE id = ?1",
        [preserved],
    )?;
    let payload = serde_json::json!({"staged": [{"staged": shared}]});
    assert!(
        history
            .cleanup_staged(vec![payload.to_string()], &images)
            .is_err()
    );
    assert!(shared.is_file());
    Ok(())
}

#[test]
fn edited_text_stays_saved_when_old_image_cleanup_fails() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let blocked = images.join("blocked.png");
    std::fs::create_dir_all(&blocked)?;
    let id = history.capture("before")?.unwrap();
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
        params![blocked.to_string_lossy(), id],
    )?;
    let hash = history.item(id)?.content_hash;
    assert!(history.edit_text(id, &hash, "after", &images)?);
    assert_eq!(history.text(id)?, "after");
    assert!(history.item(id)?.image_path.is_none());
    assert!(blocked.is_dir());
    Ok(())
}

#[test]
fn image_and_text_share_the_same_reorder_flow() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let image = history.capture_image(PNG, 3, 2, &directory.path().join("images"))?;
    let text = history.capture("text")?.unwrap();
    assert_eq!(history.list("", crate::PAGE_SIZE, false)?[0].id, text);
    history.reorder(image, text, false, false)?;
    assert_eq!(history.list("", crate::PAGE_SIZE, false)?[0].id, image);
    drop(history);
    assert_eq!(
        History::open(path)?.list("", crate::PAGE_SIZE, false)?[0].id,
        image
    );
    Ok(())
}

#[test]
fn text_eviction_removes_the_oldest_managed_image() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let history = History::new(&db);
    let images = directory.path().join("images");
    let image = history.capture_image(PNG, 3, 2, &images)?;
    let path = PathBuf::from(history.item(image)?.image_path.unwrap());
    // 未保存策略默认无限；本测验证条数上限触发的媒体回收。
    history.save_retention(
        crate::retention::RetentionPolicy {
            days: 0,
            max_count: 10_000,
        },
        true,
        &images,
    )?;
    let connection = db.write_connection();
    connection.lock().execute_batch(
        "WITH RECURSIVE nums(n) AS (
               SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 9999
             )
             INSERT INTO clipboard_items
               (content_type, text_content, content_hash, semantic_hash, created_at)
             SELECT 'text', printf('filler %d', n), printf('filler-hash-%d', n),
                    printf('filler-hash-%d', n), '9999-12-31' FROM nums;",
    )?;
    drop(connection);
    history.capture_with_media("new text", &images)?;
    assert!(history.item(image).is_err());
    assert!(!path.exists());
    Ok(())
}
