use super::*;
use crate::{database::SettingsRepository, preferences::Preferences};

fn set_payload(history: &History, id: i64, payload: &str) -> Result<()> {
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
        params![payload, id],
    )?;
    Ok(())
}

fn settings_snapshot(history: &History) -> Result<Vec<(String, String, Option<String>)>> {
    Ok(history.db.read_connection().lock().prepare(
        "SELECT key, value, updated_at FROM settings WHERE key != 'gpui_stage_files' ORDER BY key",
    )?.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}

#[test]
fn cleanup_preserves_history_metadata_other_settings_and_image_storage() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("clipboard.db");
    let history = History::open(db_path.clone())?;
    let images = dir.path().join("images");
    let staged = dir.path().join("staged");
    fs::create_dir_all(staged.join("legacy-hash/nested"))?;
    let shared = staged.join("shared.txt");
    fs::write(&shared, b"shared copy")?;
    fs::write(staged.join("legacy-hash/nested/file.txt"), b"old copy")?;
    let original = dir.path().join("original.txt");
    fs::write(&original, b"original bytes")?;
    let file = history.capture_files(&[original.to_string_lossy().into_owned()], &images)?;
    let missing = history.capture_files(
        &[dir
            .path()
            .join("missing.txt")
            .to_string_lossy()
            .into_owned()],
        &images,
    )?;
    let text = history.capture("retained text")?.unwrap();
    let image = history.capture_image(b"\x89PNG\r\n\x1a\nretained-image", 1, 1, &images)?;
    let payload = serde_json::json!({
        "staged": [{"staged": shared, "original": original}],
        "legacy": {"preferred_drop_effect": 2, "label": "kept"}
    })
    .to_string();
    for id in [file, missing, text] {
        set_payload(&history, id, &payload)?;
    }
    let group = history.create_group("kept group")?;
    history.move_to_group(file, None, Some(group.id))?;
    history.toggle_pin(file)?;
    history.toggle_favorite(file)?;
    history.set_note(file, "kept note")?;
    history.set_source_app(file, "kept app", Some("kept-icon"))?;
    let settings = SettingsRepository::new(&history.db);
    settings.set("gpui_stage_files", "true")?;
    settings.set("legacy_setting", "preserve me")?;
    Preferences::new(&history.db).set_capture_paused(true)?;
    history.db.write_connection().lock().execute_batch(
        "DROP TRIGGER clipboard_items_update_timestamp;
         UPDATE clipboard_items SET created_at = '2001-02-03 04:05:06',
             updated_at = '2002-03-04 05:06:07', last_accessed_at = '2003-04-05 06:07:08',
             access_count = 17;
         CREATE TRIGGER clipboard_items_update_timestamp AFTER UPDATE ON clipboard_items
         BEGIN UPDATE clipboard_items SET updated_at = datetime('now', 'localtime') WHERE id = new.id; END;",
    )?;
    for folder in ["blobs", "image_tmp"] {
        fs::create_dir(dir.path().join(folder))?;
        fs::write(dir.path().join(folder).join("retained"), folder.as_bytes())?;
    }
    let before = [file, missing, text, image]
        .map(|id| history.item(id))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let settings_before = settings_snapshot(&history)?;
    history.remove_file_copies(dir.path())?;
    assert!(!staged.exists());
    assert_eq!(fs::read(&original)?, b"original bytes");
    assert_eq!(settings_snapshot(&history)?, settings_before);
    assert_eq!(settings.get("gpui_stage_files")?, None);
    for mut item in before {
        let actual = history.item(item.id)?;
        if item.file_payload.is_some() {
            item.file_payload = Some(
                serde_json::json!({
                    "legacy": {"preferred_drop_effect": 2, "label": "kept"}
                })
                .to_string(),
            );
        }
        assert_eq!(serde_json::to_value(actual)?, serde_json::to_value(item)?);
    }
    assert_eq!(
        fs::read(history.item(image)?.image_path.unwrap())?,
        b"\x89PNG\r\n\x1a\nretained-image"
    );
    assert_eq!(fs::read(dir.path().join("blobs/retained"))?, b"blobs");
    assert_eq!(
        fs::read(dir.path().join("image_tmp/retained"))?,
        b"image_tmp"
    );
    assert_eq!(history.groups()?[0].name, "kept group");
    history.set_note(text, "later edit")?;
    assert_ne!(history.item(text)?.updated_at, "2002-03-04 05:06:07");
    drop(settings);
    drop(history);
    let reopened = History::open(db_path)?;
    reopened.remove_file_copies(dir.path())?;
    assert_eq!(
        reopened.files(file)?,
        vec![original.to_string_lossy().into_owned()]
    );
    assert!(!staged.exists());
    Ok(())
}

#[test]
fn database_failure_rolls_back_all_references_settings_and_trigger_before_disk_cleanup()
-> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let staged = dir.path().join("staged");
    fs::create_dir(&staged)?;
    fs::write(staged.join("copy"), b"still owned")?;
    let ids = [
        history.capture("first")?.unwrap(),
        history.capture("second")?.unwrap(),
    ];
    let payload = r#"{"staged":[{"staged":"unused path"}],"legacy":true}"#;
    for id in ids {
        set_payload(&history, id, payload)?;
    }
    SettingsRepository::new(&history.db).set("gpui_stage_files", "true")?;
    history.db.write_connection().lock().execute_batch(
        "CREATE TRIGGER reject_cleanup BEFORE UPDATE OF file_payload ON clipboard_items
         WHEN new.id = (SELECT MAX(id) FROM clipboard_items)
         BEGIN SELECT RAISE(ABORT, 'injected cleanup failure'); END;",
    )?;
    let before = ids
        .map(|id| history.item(id))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    assert!(history.remove_file_copies(dir.path()).is_err());
    assert_eq!(fs::read(staged.join("copy"))?, b"still owned");
    assert_eq!(
        SettingsRepository::new(&history.db)
            .get("gpui_stage_files")?
            .as_deref(),
        Some("true")
    );
    for expected in before {
        assert_eq!(
            serde_json::to_value(history.item(expected.id)?)?,
            serde_json::to_value(expected)?
        );
    }
    let trigger_exists: bool = history.db.read_connection().lock().query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='clipboard_items_update_timestamp')",
        [],
        |row| row.get(0),
    )?;
    assert!(trigger_exists);
    history
        .db
        .write_connection()
        .lock()
        .execute_batch("DROP TRIGGER reject_cleanup;")?;
    history.remove_file_copies(dir.path())?;
    assert!(!staged.exists());
    for id in ids {
        assert_eq!(
            history.item(id)?.file_payload.as_deref(),
            Some(r#"{"legacy":true}"#)
        );
    }
    Ok(())
}

#[test]
fn malformed_payload_rolls_back_valid_rows_and_does_not_remove_files() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    fs::create_dir(dir.path().join("staged"))?;
    fs::write(dir.path().join("staged/copy"), b"keep")?;
    let first = history.capture("first")?.unwrap();
    let second = history.capture("second")?.unwrap();
    let valid = r#"{"staged":[1],"legacy":false}"#;
    set_payload(&history, first, valid)?;
    set_payload(&history, second, "{invalid JSON")?;
    assert!(history.remove_file_copies(dir.path()).is_err());
    assert_eq!(history.item(first)?.file_payload.as_deref(), Some(valid));
    assert_eq!(fs::read(dir.path().join("staged/copy"))?, b"keep");
    set_payload(&history, second, "{}")?;
    history.remove_file_copies(dir.path())?;
    assert_eq!(
        history.item(first)?.file_payload.as_deref(),
        Some(r#"{"legacy":false}"#)
    );
    Ok(())
}

#[test]
fn payload_cleanup_preserves_unknown_legacy_values_and_clears_duplicate_staged_keys() -> Result<()>
{
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let id = history.capture("legacy payload")?.unwrap();
    let raw = r#" {"large":18446744073709551616000,"staged":[{"staged":"outside"}],"precise":1.234567890123456789,"staged":false,"legacy":{"staged":"nested stays"},"large":2} "#;
    set_payload(&history, id, raw)?;
    history.remove_file_copies(dir.path())?;
    assert_eq!(
        history.item(id)?.file_payload.as_deref(),
        Some(
            r#"{"large":18446744073709551616000,"precise":1.234567890123456789,"legacy":{"staged":"nested stays"},"large":2}"#
        )
    );
    for unchanged in [r#" {"legacy": [1, true, null]} "#, "null", r#"["legacy"]"#] {
        set_payload(&history, id, unchanged)?;
        history.remove_file_copies(dir.path())?;
        assert_eq!(history.item(id)?.file_payload.as_deref(), Some(unchanged));
    }
    set_payload(&history, id, r#"{"staged":null}"#)?;
    history.remove_file_copies(dir.path())?;
    assert_eq!(history.item(id)?.file_payload.as_deref(), Some("{}"));
    assert!(!dir.path().join("staged").exists());
    Ok(())
}

#[test]
fn cleanup_ignores_payload_paths_and_keeps_other_profiles_and_their_references() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let profile = dir.path().join("profile");
    let other = dir.path().join("other-profile");
    let history = History::open(profile.join("clipboard.db"))?;
    let second = History::open(other.join("clipboard.db"))?;
    for root in [&profile, &other] {
        fs::create_dir(root.join("staged"))?;
        fs::write(root.join("staged/copy"), b"copy")?;
    }
    let external = dir.path().join("outside");
    fs::write(&external, b"outside bytes")?;
    let payload = serde_json::json!({"staged": [
        {"staged": external}, {"staged": other.join("staged/copy")},
        {"staged": profile.join("staged/../../outside")}
    ], "legacy": "keep"})
    .to_string();
    let id = history.capture("first profile")?.unwrap();
    let other_id = second.capture("second profile")?.unwrap();
    set_payload(&history, id, &payload)?;
    set_payload(&second, other_id, &payload)?;
    assert!(history.remove_file_copies(&other).is_err());
    assert_eq!(
        history.item(id)?.file_payload.as_deref(),
        Some(payload.as_str())
    );
    history.remove_file_copies(&profile)?;
    assert!(!profile.join("staged").exists());
    assert_eq!(fs::read(external)?, b"outside bytes");
    assert_eq!(fs::read(other.join("staged/copy"))?, b"copy");
    assert_eq!(
        second.item(other_id)?.file_payload.as_deref(),
        Some(payload.as_str())
    );
    Ok(())
}

#[test]
fn capture_retains_paths_without_making_copies_or_requiring_source_contents() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let folder = dir.path().join("folder");
    fs::create_dir(&folder)?;
    let source = folder.join("source.txt");
    fs::write(&source, b"original")?;
    let paths = vec![
        source.to_string_lossy().into_owned(),
        folder.to_string_lossy().into_owned(),
        dir.path().join("missing").to_string_lossy().into_owned(),
    ];
    let id = history.capture_files(&paths, &dir.path().join("images"))?;
    fs::write(&source, b"updated original")?;
    assert_eq!(
        history.capture_files(&paths, &dir.path().join("images"))?,
        id
    );
    assert_eq!(history.files(id)?, paths);
    assert!(history.item(id)?.file_payload.is_none());
    assert!(!dir.path().join("staged").exists());
    assert_eq!(fs::read(&source)?, b"updated original");
    fs::remove_file(source)?;
    assert_eq!(History::file_paths_for_copy(&history.item(id)?)?, paths);
    Ok(())
}

#[test]
fn database_only_cleanup_supports_old_schemas_without_migrating_them() -> Result<()> {
    for settings_present in [false, true] {
        let mut connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE TABLE clipboard_items(id INTEGER PRIMARY KEY, file_paths TEXT, updated_at TEXT);
             INSERT INTO clipboard_items VALUES (1, '[\"original\"]', 'old timestamp');",
        )?;
        if settings_present {
            connection.execute_batch(
                "CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);
                 INSERT INTO settings VALUES ('gpui_stage_files', 'true'), ('legacy', 'keep');",
            )?;
        }
        clear_file_copy_references(&mut connection)?;
        let row: (String, String) = connection.query_row(
            "SELECT file_paths, updated_at FROM clipboard_items",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            row,
            (r#"["original"]"#.to_owned(), "old timestamp".to_owned())
        );
        let columns: i64 = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clipboard_items')",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(columns, 3);
        if settings_present {
            let settings: Vec<(String, String)> = connection
                .prepare("SELECT key, value FROM settings")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            assert_eq!(settings, vec![("legacy".into(), "keep".into())]);
        }
    }
    Ok(())
}

#[cfg(any(unix, windows))]
mod paths;
