use super::*;

fn executable(version: &str) -> PathBuf {
    std::env::temp_dir()
        .join(version)
        .join("elegant-clipboard-gpui")
}

#[test]
fn relocated_entry_keeps_disabled_state_and_previous_executable_is_accepted() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().join("config");
    let old_dir = temporary.path().join("removed GPUI 数据 %`$\"\\");
    let new_dir = temporary.path().join("ElegantClipboard");
    fs::create_dir(&new_dir).unwrap();
    let old = entry_path(&base, &old_dir).unwrap();
    let new = entry_path(&base, &new_dir).unwrap();
    let saved =
        entry(&executable("previous"), &old_dir).unwrap() + "Hidden=true\nOnlyShowIn=GNOME;\n";
    file::write(&old, &saved).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert!(!old.exists());
    assert!(!old_dir.exists());
    let migrated = file::read(&new).unwrap().unwrap();
    assert!(!desktop::parse(&migrated, &new_dir).unwrap().enabled);
    assert!(
        migrated
            .lines()
            .any(|line| line == desktop::exec(&executable("current"), &new_dir).unwrap())
    );
    assert!(migrated.contains("OnlyShowIn=GNOME;"));
    migrate_at(&base, &old_dir, &new_dir, &executable("next-version")).unwrap();
    assert_eq!(file::read(&new).unwrap().unwrap(), migrated);
}

#[test]
fn absent_entry_and_other_application_do_not_create_autostart() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().join("config");
    let old_dir = temporary.path().join("old");
    let new_dir = temporary.path().join("new");
    let old = entry_path(&base, &old_dir).unwrap();
    let new = entry_path(&base, &new_dir).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert!(!new.exists());
    let another_app = entry(&temporary.path().join("other-app"), &old_dir).unwrap();
    file::write(&old, &another_app).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert_eq!(file::read(&old).unwrap().unwrap(), another_app);
    assert!(!new.exists());
    let wrong_profile = entry(&executable("old"), &new_dir).unwrap();
    file::write(&old, &wrong_profile).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert_eq!(file::read(&old).unwrap().unwrap(), wrong_profile);
    assert!(!new.exists());
}

#[test]
fn retry_keeps_existing_new_entry_and_finishes_removing_old_entry() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let old_dir = base.join("old");
    let new_dir = base.join("new");
    let old = entry_path(base, &old_dir).unwrap();
    let new = entry_path(base, &new_dir).unwrap();
    file::write(&old, &entry(&executable("old"), &old_dir).unwrap()).unwrap();
    let destination = entry(&executable("user-update"), &new_dir).unwrap() + "Hidden=true\n";
    file::write(&new, &destination).unwrap();
    migrate_at(base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert_eq!(file::read(&new).unwrap().unwrap(), destination);
    assert!(!old.exists());
}

#[test]
fn destination_conflict_or_write_failure_retains_old_entry_for_retry() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let old_dir = base.join("old");
    let new_dir = base.join("new");
    let old = entry_path(base, &old_dir).unwrap();
    let new = entry_path(base, &new_dir).unwrap();
    let saved = entry(&executable("old"), &old_dir).unwrap();
    file::write(&old, &saved).unwrap();
    file::write(&new, "[Desktop Entry]\nExec=another-app\n").unwrap();
    assert!(migrate_at(base, &old_dir, &new_dir, &executable("current")).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
    fs::remove_file(&new).unwrap();
    fs::create_dir(&new).unwrap();
    assert!(migrate_at(base, &old_dir, &new_dir, &executable("current")).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
    fs::remove_dir(&new).unwrap();
    let blocked = base.join("blocked");
    fs::write(&blocked, "file").unwrap();
    assert!(file::migrate(&old, &blocked.join("new.desktop"), &saved, &saved).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
}

#[test]
fn duplicate_exec_or_unexpanded_field_code_is_not_an_owned_entry() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    let content = entry(&executable("old"), &data_dir).unwrap();
    assert!(desktop::parse(&(content.clone() + "Exec=other\n"), &data_dir).is_none());
    assert!(
        desktop::parse(
            &content.replace(" --start-hidden", " %u --start-hidden"),
            &data_dir
        )
        .is_none()
    );
}
