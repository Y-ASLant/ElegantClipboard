use super::*;
use crate::{
    NativeFormatCapture, PAGE_SIZE, backup::restore_backup, import::import_legacy_database,
};

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn snapshot_names_roundtrip_and_replace_without_changing_payloads() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let id = history.capture("host text")?.unwrap();
    history.attach_native_formats(
        id,
        vec![NativeFormatCapture {
            name: "PayloadOnly".to_owned(),
            data: b"restorable bytes".to_vec(),
        }],
    )?;
    assert!(history.item(id)?.formats.is_empty());
    history.set_captured_formats(
        id,
        &names(&["public.utf8-plain-text", "CF_UNICODETEXT", "CF_UNICODETEXT"]),
    )?;
    let expected = names(&["CF_UNICODETEXT", "public.utf8-plain-text"]);
    assert_eq!(history.item(id)?.formats, expected);
    assert_eq!(history.list("", PAGE_SIZE, false)?[0].formats, expected);
    drop(history);

    let history = History::open(path)?;
    assert_eq!(history.item(id)?.formats, expected);
    assert_eq!(history.capture("host text")?, Some(id));
    history.set_captured_formats(id, &names(&["text/plain;charset=utf-8"]))?;
    assert_eq!(
        history.item(id)?.formats,
        names(&["text/plain;charset=utf-8"])
    );
    history.set_captured_formats(id, &[])?;
    assert!(history.item(id)?.formats.is_empty());
    assert_eq!(
        history.native_formats_for_copy(id)?,
        vec![("PayloadOnly".to_owned(), b"restorable bytes".to_vec())]
    );
    Ok(())
}

#[test]
fn invalid_snapshots_do_not_partially_replace_previous_names() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("host")?.unwrap();
    let original = names(&[" Native Name ", "native name"]);
    history.set_captured_formats(id, &original)?;
    for invalid in [
        "".to_owned(),
        " ".to_owned(),
        "bad\0name".to_owned(),
        "x".repeat(257),
    ] {
        assert!(
            history
                .set_captured_formats(id, &["valid".to_owned(), invalid])
                .is_err()
        );
        assert_eq!(history.item(id)?.formats, original);
    }
    assert!(
        history
            .set_captured_formats(id, &vec!["Format".to_owned(); MAX_CAPTURED_FORMATS + 1])
            .is_err()
    );
    assert_eq!(history.item(id)?.formats, original);
    assert!(history.set_captured_formats(id + 1, &original).is_err());
    assert!(history.set_captured_formats(id + 1, &[]).is_err());
    Ok(())
}

#[test]
fn old_database_remains_unknown_despite_native_payloads() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let id = history.capture("legacy")?.unwrap();
    history.attach_native_formats(
        id,
        vec![NativeFormatCapture {
            name: "HTML Format".to_owned(),
            data: b"old payload".to_vec(),
        }],
    )?;
    history.db.write_connection().lock().execute_batch(
        "DROP TRIGGER clipboard_items_invalidate_captured_formats;
         DROP TABLE captured_formats;",
    )?;
    drop(history);
    let imported_path = directory.path().join("imported/clipboard.db");
    import_legacy_database(&path, &imported_path)?;
    let imported = History::open(imported_path)?;
    assert!(imported.item(id)?.formats.is_empty());
    assert_eq!(imported.native_formats_for_copy(id)?.len(), 1);

    let history = History::open(path)?;
    assert!(history.item(id)?.formats.is_empty());
    assert!(history.list("", PAGE_SIZE, false)?[0].formats.is_empty());
    assert_eq!(history.native_formats_for_copy(id)?.len(), 1);
    history.set_captured_formats(id, &names(&["CF_UNICODETEXT"]))?;
    assert_eq!(history.item(id)?.formats, names(&["CF_UNICODETEXT"]));
    Ok(())
}

#[test]
fn content_edits_invalidate_names_but_noops_and_metadata_changes_do_not() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let id = history.capture_rich(Some("<b>hello</b>"), None, Some("hello"), directory.path())?;
    let expected = names(&["HTML Format", "CF_UNICODETEXT"]);
    history.set_captured_formats(id, &expected)?;
    let expected = history.item(id)?.formats;
    let hash = history.item(id)?.content_hash;
    history.toggle_pin(id)?;
    history.toggle_favorite(id)?;
    assert!(!history.edit_text(id, &hash, "hello", directory.path())?);
    assert!(
        history
            .edit_text(id, "stale", "changed", directory.path())
            .is_err()
    );
    assert_eq!(history.item(id)?.formats, expected);
    assert!(history.edit_text(id, &hash, "changed", directory.path())?);
    assert!(history.item(id)?.formats.is_empty());
    drop(history);
    assert!(History::open(path)?.item(id)?.formats.is_empty());
    Ok(())
}

#[test]
fn native_backup_and_import_keep_source_platform_names() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source/clipboard.db");
    let history = History::open(source.clone())?;
    let id = history.capture("source platform")?.unwrap();
    let expected = names(&[
        "UTF8_STRING",
        "public.utf8-plain-text",
        "text/plain;charset=utf-8",
    ]);
    history.set_captured_formats(id, &expected)?;
    let unknown = history.capture("without captured metadata")?.unwrap();
    let image = history.capture_image(
        b"\x89PNG\r\n\x1a\nsynthetic",
        2,
        3,
        &directory.path().join("source/images"),
    )?;
    history.set_captured_formats(image, &names(&["CF_DIBV5"]))?;
    let archive = directory.path().join("backup.zip");
    history.export_backup(&archive)?;
    let target = directory.path().join("restored");
    std::fs::create_dir_all(&target)?;
    restore_backup(&archive, &target)?;
    let restored = History::open(target.join("clipboard.db"))?;
    assert_eq!(restored.item(id)?.formats, expected);
    assert!(restored.item(unknown)?.formats.is_empty());
    assert_eq!(restored.item(image)?.formats, names(&["CF_DIBV5"]));
    assert_ne!(
        restored.item(image)?.image_path,
        history.item(image)?.image_path
    );

    let imported_path = directory.path().join("imported/clipboard.db");
    import_legacy_database(&source, &imported_path)?;
    let imported = History::open(imported_path)?;
    assert_eq!(imported.item(id)?.formats, expected);
    assert!(imported.item(unknown)?.formats.is_empty());
    assert_eq!(imported.item(image)?.formats, names(&["CF_DIBV5"]));
    Ok(())
}

#[test]
fn deleting_items_cascades_capture_metadata() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let first = history.capture("first")?.unwrap();
    let second = history.capture("second")?.unwrap();
    history.set_captured_formats(first, &names(&["CF_UNICODETEXT"]))?;
    history.set_captured_formats(second, &names(&["UTF8_STRING"]))?;
    history.delete_with_media(first, directory.path())?;
    let connection = history.db.read_connection();
    let count: i64 =
        connection
            .lock()
            .query_row("SELECT COUNT(*) FROM captured_formats", [], |row| {
                row.get(0)
            })?;
    assert_eq!(count, 1);
    assert_eq!(history.item(second)?.formats, names(&["UTF8_STRING"]));
    history.clear_all_with_media(directory.path())?;
    let count: i64 =
        connection
            .lock()
            .query_row("SELECT COUNT(*) FROM captured_formats", [], |row| {
                row.get(0)
            })?;
    assert_eq!(count, 0);
    Ok(())
}
