use super::*;
use crate::preferences::Preferences;

fn staging_tempdir() -> Result<tempfile::TempDir> {
    // macOS exposes its temporary directory through the /var symlink.
    // Ordinary-file fixtures must not exercise the link rejection policy.
    let root = fs::canonicalize(std::env::temp_dir())?;
    Ok(tempfile::tempdir_in(root)?)
}

fn copied_path(history: &History, id: i64) -> Result<PathBuf> {
    let item = history.item(id)?;
    let payload: serde_json::Value = serde_json::from_str(item.file_payload.as_deref().unwrap())?;
    Ok(PathBuf::from(
        payload["staged"][0]["staged"].as_str().unwrap(),
    ))
}

#[test]
fn file_staging_recovers_missing_original_and_disabling_keeps_existing_copy() -> Result<()> {
    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let source = directory.path().join("sample.txt");
    fs::write(&source, b"original bytes")?;
    let paths = vec![source.to_string_lossy().into_owned()];
    let preferences = Preferences::new(&history.db);
    assert!(!preferences.stage_files()?);
    preferences.set_stage_files(true)?;
    let outcome = history.capture_files_with_staging(&paths, &images)?;
    assert!(outcome.warnings.is_empty());
    let copy = copied_path(&history, outcome.id)?;
    assert_eq!(
        History::file_paths_for_copy(&history.item(outcome.id)?, &directory.path().join("staged"))?,
        paths
    );
    preferences.set_stage_files(false)?;
    assert_eq!(history.capture_files(&paths, &images)?, outcome.id);
    assert_eq!(copied_path(&history, outcome.id)?, copy);
    fs::remove_file(&source)?;
    let resolved =
        History::file_paths_for_copy(&history.item(outcome.id)?, &directory.path().join("staged"))?;
    assert_eq!(fs::read(&resolved[0])?, b"original bytes");
    assert!(!preferences.stage_files()?);
    history.delete_with_media(outcome.id, &images)?;
    assert!(!copy.exists());
    Ok(())
}

#[test]
fn file_staging_refreshes_same_path_and_preserves_shared_old_copy() -> Result<()> {
    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let source = directory.path().join("sample.txt");
    fs::write(&source, b"first version")?;
    let paths = vec![source.to_string_lossy().into_owned()];
    let first = history.capture_files_with_staging(&paths, &images)?;
    let old = copied_path(&history, first.id)?;
    let shared = history.capture_files(&["missing-shared.txt".into()], &images)?;
    let payload = history.item(first.id)?.file_payload;
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
        params![payload, shared],
    )?;
    fs::write(&source, b"second version")?;
    let second = history.capture_files_with_staging(&paths, &images)?;
    assert_eq!(second.id, first.id);
    let new = copied_path(&history, second.id)?;
    assert_ne!(new, old);
    assert_eq!(fs::read(&new)?, b"second version");
    assert_eq!(fs::read(&old)?, b"first version");
    history.delete_with_media(shared, &images)?;
    assert!(!old.exists());
    assert!(new.exists());
    fs::write(&source, b"third version")?;
    history.capture_files_with_staging(&paths, &images)?;
    assert!(!new.exists());
    Ok(())
}

#[test]
fn file_staging_budget_and_file_limit_fall_back_to_original_paths() -> Result<()> {
    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let staged = directory.path().join("staged");
    fs::create_dir(&staged)?;
    let reserved = staged.join("already-used.bin");
    File::create(&reserved)?.set_len(MAX_STAGED_DIRECTORY_BYTES - 3)?;
    let source = directory.path().join("sample.txt");
    fs::write(&source, b"fits")?;
    let paths = vec![source.to_string_lossy().into_owned()];
    let outcome = history.capture_files_with_staging(&paths, &images)?;
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(history.files(outcome.id)?, paths);
    assert_eq!(fs::read_dir(&staged)?.count(), 1);
    fs::remove_file(&reserved)?;
    File::create(&source)?.set_len(MAX_STAGED_FILE_BYTES + 1)?;
    let oversized = history.capture_files_with_staging(&paths, &images)?;
    assert_eq!(oversized.warnings.len(), 1);
    assert_eq!(fs::read_dir(&staged)?.count(), 0);
    fs::write(&source, b"fits")?;
    let recovered = history.capture_files_with_staging(&paths, &images)?;
    assert!(recovered.warnings.is_empty());
    assert_eq!(fs::read(copied_path(&history, recovered.id)?)?, b"fits");
    Ok(())
}

#[test]
fn file_staging_db_failure_rolls_back_new_copies_and_keeps_previous_payload() -> Result<()> {
    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let staged = directory.path().join("staged");
    let source = directory.path().join("sample.txt");
    fs::write(&source, b"first")?;
    let paths = vec![source.to_string_lossy().into_owned()];
    let first = history.capture_files_with_staging(&paths, &images)?;
    let old = copied_path(&history, first.id)?;
    history.db.write_connection().lock().execute_batch(
        "CREATE TRIGGER reject_update BEFORE UPDATE ON clipboard_items
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END;
         CREATE TRIGGER reject_insert BEFORE INSERT ON clipboard_items
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
    )?;
    fs::write(&source, b"second")?;
    assert!(history.capture_files_with_staging(&paths, &images).is_err());
    assert_eq!(copied_path(&history, first.id)?, old);
    assert_eq!(fs::read(&old)?, b"first");
    assert_eq!(fs::read_dir(&staged)?.count(), 1);
    let second = directory.path().join("other.txt");
    fs::write(&second, b"other")?;
    assert!(
        history
            .capture_files_with_staging(&[second.to_string_lossy().into_owned()], &images)
            .is_err()
    );
    assert_eq!(fs::read_dir(&staged)?.count(), 1);
    Ok(())
}

#[test]
fn file_staging_rejects_directories_missing_files_and_unsafe_staged_contents() -> Result<()> {
    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let folder = directory.path().join("folder");
    fs::create_dir(&folder)?;
    fs::write(folder.join("nested.txt"), b"do not recurse")?;
    let paths = vec![
        folder.to_string_lossy().into_owned(),
        directory
            .path()
            .join("missing")
            .to_string_lossy()
            .into_owned(),
    ];
    let result = history.capture_files_with_staging(&paths, &images)?;
    assert_eq!(result.warnings.len(), 2);
    assert_eq!(history.files(result.id)?, paths);
    assert_eq!(fs::read_dir(directory.path().join("staged"))?.count(), 0);
    fs::create_dir(directory.path().join("staged/nested"))?;
    let source = folder.join("nested.txt");
    let result =
        history.capture_files_with_staging(&[source.to_string_lossy().into_owned()], &images)?;
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(fs::read_dir(directory.path().join("staged"))?.count(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_staging_rejects_symlink_sources_and_data_directories() -> Result<()> {
    use std::os::unix::fs::symlink;

    let directory = staging_tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let source = directory.path().join("sample.txt");
    fs::write(&source, b"original bytes")?;
    let linked_file = directory.path().join("linked.txt");
    symlink(&source, &linked_file)?;
    let linked_directory = directory.path().join("linked-directory");
    symlink(directory.path(), &linked_directory)?;

    for path in [linked_file, linked_directory.join("sample.txt")] {
        let paths = vec![path.to_string_lossy().into_owned()];
        let outcome = history.capture_files_with_staging(&paths, &images)?;
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(history.files(outcome.id)?, paths);
        assert_eq!(fs::read_dir(directory.path().join("staged"))?.count(), 0);
    }

    let paths = vec![source.to_string_lossy().into_owned()];
    let outcome = history.capture_files_with_staging(&paths, &linked_directory.join("images"))?;
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(history.files(outcome.id)?, paths);
    assert_eq!(fs::read_dir(directory.path().join("staged"))?.count(), 0);
    assert_eq!(fs::read(source)?, b"original bytes");
    Ok(())
}
