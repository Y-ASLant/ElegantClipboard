use super::*;

#[test]
fn settings_get_set() {
    let db = temp_db();
    let repo = SettingsRepository::new(&db);
    repo.set("my_key", "my_value").unwrap();
    assert_eq!(repo.get("my_key").unwrap(), Some("my_value".to_string()));
}

#[test]
fn settings_get_nonexistent() {
    let db = temp_db();
    let repo = SettingsRepository::new(&db);
    assert_eq!(repo.get("no_such_key").unwrap(), None);
}

#[test]
fn settings_get_all() {
    let db = temp_db();
    let repo = SettingsRepository::new(&db);
    let all = repo.get_all().unwrap();
    assert!(all.contains_key("global_shortcut"));
}

#[test]
fn settings_clear_all() {
    let db = temp_db();
    let repo = SettingsRepository::new(&db);
    repo.set("extra", "val").unwrap();
    repo.clear_all().unwrap();
    assert!(repo.get_all().unwrap().is_empty());
}

#[test]
fn settings_upsert() {
    let db = temp_db();
    let repo = SettingsRepository::new(&db);
    repo.set("upsert_key", "first").unwrap();
    repo.set("upsert_key", "second").unwrap();
    assert_eq!(repo.get("upsert_key").unwrap(), Some("second".to_string()));
}
