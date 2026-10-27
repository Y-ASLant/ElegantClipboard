use super::*;
use clipboard_core::{
    History,
    database::Database,
    preferences::{Preferences, ThemePreference},
};
use std::fs;

fn paths(root: &Path) -> DefaultPaths {
    DefaultPaths {
        current: root.join("ElegantClipboard"),
        legacy: root.join("ASLant/ElegantClipboard-GPUI/data"),
    }
}

fn seed(directory: &Path, text: &str) -> Result<i64> {
    fs::create_dir_all(directory)?;
    let history = History::open(directory.join("clipboard.db"))?;
    let id = history.capture(text)?.unwrap();
    history.set_note(id, "迁移后保留备注")?;
    history.toggle_favorite(id)?;
    history.toggle_pin(id)?;
    Ok(id)
}

#[test]
fn migration_preserves_data_and_unknown_files_then_cleans_only_empty_legacy_parents() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    let id = seed(&paths.legacy, "old history")?;
    let db = Database::new(paths.legacy.join("clipboard.db"))?;
    Preferences::new(&db).set_theme(ThemePreference::Dark)?;
    drop(db);
    fs::write(paths.legacy.join("user-kept.txt"), "preserve me")?;
    fs::write(directory.path().join("ASLant/other-app.txt"), "unrelated")?;
    let prepared = migration::prepare_default(&paths, |old, new| {
        assert!(!old.exists());
        assert!(new.join("clipboard.db").is_file());
        Ok(())
    })?;
    assert!(prepared.warning.is_none());
    assert_eq!(prepared.path, paths.current);
    assert!(!paths.legacy.exists());
    assert!(!paths.legacy.parent().unwrap().exists());
    assert!(directory.path().join("ASLant/other-app.txt").is_file());
    assert_eq!(
        fs::read_to_string(paths.current.join("user-kept.txt"))?,
        "preserve me"
    );
    let history = History::open(paths.current.join("clipboard.db"))?;
    let item = history.item(id)?;
    assert_eq!(item.text_content.as_deref(), Some("old history"));
    assert_eq!(item.note.as_deref(), Some("迁移后保留备注"));
    assert!(item.is_favorite && item.is_pinned);
    let db = Database::new(paths.current.join("clipboard.db"))?;
    assert_eq!(Preferences::new(&db).theme()?, ThemePreference::Dark);
    drop(db);
    drop(history);
    drop(prepared);
    let again =
        migration::prepare_default(&paths, |_, _| panic!("completed migration must not replay"))?;
    assert!(again.warning.is_none());
    Ok(())
}

#[test]
fn empty_legacy_profile_is_removed_without_touching_other_organization_data() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let profiles = paths(directory.path());
    drop(lock_instance(&profiles.legacy)?);
    let sibling = directory.path().join("ASLant/another-app");
    fs::create_dir_all(&sibling)?;
    fs::write(sibling.join("data.bin"), b"keep")?;
    let prepared = migration::prepare_default(&profiles, |old, new| {
        assert!(old.join("instance.lock").is_file());
        assert!(new.join("instance.lock").is_file());
        Ok(())
    })?;
    assert!(prepared.warning.is_none());
    assert!(!profiles.legacy.exists());
    assert_eq!(fs::read(sibling.join("data.bin"))?, b"keep");
    Ok(())
}

#[test]
fn populated_new_and_old_profiles_are_never_merged_or_deleted() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    seed(&paths.legacy, "legacy")?;
    seed(&paths.current, "current")?;
    let prepared = migration::prepare_default(&paths, |_, _| {
        panic!("conflicting data must not change startup entries")
    })?;
    assert!(prepared.warning.is_some());
    assert_eq!(
        History::open(paths.current.join("clipboard.db"))?.list("", 100, false)?[0]
            .preview
            .as_deref(),
        Some("current")
    );
    assert_eq!(
        History::open(paths.legacy.join("clipboard.db"))?.list("", 100, false)?[0]
            .preview
            .as_deref(),
        Some("legacy")
    );
    Ok(())
}

#[test]
fn active_legacy_profile_is_not_moved_and_a_later_retry_succeeds() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    seed(&paths.legacy, "locked history")?;
    let legacy_lock = lock_instance(&paths.legacy)?;
    assert!(migration::prepare_default(&paths, |_, _| Ok(())).is_err());
    assert!(paths.legacy.join("clipboard.db").is_file());
    assert!(!paths.current.exists());
    drop(legacy_lock);
    let prepared = migration::prepare_default(&paths, |_, _| Ok(()))?;
    assert!(prepared.warning.is_none());
    assert!(!paths.legacy.exists());
    Ok(())
}

#[test]
fn startup_migration_failure_leaves_a_resumable_marker_without_duplicating_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    seed(&paths.legacy, "retry history")?;
    let prepared =
        migration::prepare_default(&paths, |_, _| anyhow::bail!("startup entry is locked"))?;
    assert!(prepared.warning.is_some());
    assert!(
        paths
            .current
            .join(".data-directory-migration.json")
            .is_file()
    );
    assert!(!paths.legacy.exists());
    drop(prepared);
    let prepared = migration::prepare_default(&paths, |_, _| Ok(()))?;
    assert!(prepared.warning.is_none());
    assert!(
        !paths
            .current
            .join(".data-directory-migration.json")
            .exists()
    );
    assert_eq!(
        History::open(paths.current.join("clipboard.db"))?.count("", false)?,
        1
    );
    Ok(())
}

#[test]
fn corrupt_database_is_retained_and_can_resume_after_the_source_is_repaired() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    fs::create_dir_all(&paths.legacy)?;
    fs::write(paths.legacy.join("clipboard.db"), b"not sqlite")?;
    assert!(migration::prepare_default(&paths, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(paths.current.join("clipboard.db"))?, b"not sqlite");
    assert!(
        paths
            .current
            .join(".data-directory-migration.json")
            .exists()
    );
    fs::remove_file(paths.current.join("clipboard.db"))?;
    seed(&paths.current, "repaired history")?;
    let prepared = migration::prepare_default(&paths, |_, _| Ok(()))?;
    assert!(prepared.warning.is_none());
    assert_eq!(
        History::open(paths.current.join("clipboard.db"))?.count("", false)?,
        1
    );
    Ok(())
}

#[test]
fn empty_new_profile_can_receive_legacy_data_but_unknown_new_files_are_preserved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    seed(&paths.legacy, "legacy")?;
    drop(lock_instance(&paths.current)?);
    let prepared = migration::prepare_default(&paths, |_, _| Ok(()))?;
    assert!(prepared.warning.is_none());
    assert!(!paths.legacy.exists());
    drop(prepared);
    let other = tempfile::tempdir()?;
    let paths = self::paths(other.path());
    seed(&paths.legacy, "legacy")?;
    fs::create_dir_all(&paths.current)?;
    fs::write(paths.current.join("unknown.txt"), "preserve")?;
    let prepared = migration::prepare_default(&paths, |_, _| Ok(()))?;
    assert!(prepared.warning.is_some());
    assert!(paths.legacy.join("clipboard.db").is_file());
    assert_eq!(
        fs::read_to_string(paths.current.join("unknown.txt"))?,
        "preserve"
    );
    Ok(())
}

#[test]
fn explicit_old_default_aliases_migrate_but_custom_profiles_never_touch_default_data() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let profiles = paths(directory.path());
    seed(&profiles.legacy, "legacy")?;
    let alias = profiles.legacy.clone();
    let expected = profiles.current.clone();
    let prepared = prepare_with_paths(Some(alias.clone()), Ok(profiles), |_, _| Ok(()))?;
    assert_eq!(prepared.path, expected);
    assert!(!alias.exists());
    drop(prepared);
    let profiles = paths(directory.path());
    let custom = directory.path().join("custom");
    let prepared = prepare_with_paths(Some(custom.clone()), Ok(profiles), |_, _| {
        panic!("custom profile must not migrate defaults")
    })?;
    assert_eq!(prepared.path, custom);
    assert!(!custom.join("clipboard.db").exists());
    drop(prepared);
    let prepared = prepare_with_paths(
        Some(custom.clone()),
        Err(anyhow::anyhow!("no home directory")),
        |_, _| panic!("custom profile must not need a home directory"),
    )?;
    assert_eq!(prepared.path, custom);
    Ok(())
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_rejected_without_moving_or_cleaning_the_target() -> Result<()> {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir()?;
    let paths = paths(directory.path());
    seed(&paths.legacy, "legacy")?;
    let external = directory.path().join("outside.txt");
    fs::write(&external, "untouched")?;
    symlink(&external, paths.legacy.join("linked.txt"))?;
    assert!(migration::prepare_default(&paths, |_, _| Ok(())).is_err());
    assert!(paths.legacy.join("clipboard.db").is_file());
    assert_eq!(fs::read_to_string(external)?, "untouched");
    assert!(!paths.current.exists());
    Ok(())
}
