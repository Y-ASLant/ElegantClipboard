use super::*;

#[test]
fn settings_get_set() {
    let (_dir, db) = temp_db();
    let repo = SettingsRepository::new(&db);
    repo.set("my_key", "my_value").unwrap();
    assert_eq!(repo.get("my_key").unwrap(), Some("my_value".to_string()));
}

#[test]
fn settings_get_nonexistent() {
    let (_dir, db) = temp_db();
    let repo = SettingsRepository::new(&db);
    assert_eq!(repo.get("no_such_key").unwrap(), None);
}

#[test]
fn settings_upsert() {
    let (_dir, db) = temp_db();
    let repo = SettingsRepository::new(&db);
    repo.set("upsert_key", "first").unwrap();
    repo.set("upsert_key", "second").unwrap();
    assert_eq!(repo.get("upsert_key").unwrap(), Some("second".to_string()));
}

#[test]
fn clear_user_settings_keeps_migration_markers() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::database::Database::new(dir.path().join("t.db")).unwrap();
    let settings = crate::database::SettingsRepository::new(&db);
    settings.set("gpui_theme_mode", "dark").unwrap();
    settings.set("_migration_example", "done").unwrap();
    settings.clear_user_settings().unwrap();
    assert!(settings.get("gpui_theme_mode").unwrap().is_none());
    assert_eq!(
        settings.get("_migration_example").unwrap().as_deref(),
        Some("done")
    );
}
