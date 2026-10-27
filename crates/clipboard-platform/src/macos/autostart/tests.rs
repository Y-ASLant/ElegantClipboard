use super::*;

fn executable(version: &str) -> PathBuf {
    std::env::temp_dir()
        .join(version)
        .join("elegant-clipboard-gpui")
}

fn disabled_entry(exe: &Path, data_dir: &Path) -> String {
    entry(exe, data_dir)
        .unwrap()
        .replace("</dict>", "<key>Disabled</key><true/></dict>")
}

#[test]
fn migration_relocates_previous_executable_and_preserves_disabled_launch_agent() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().join("LaunchAgents");
    let old_dir = temporary.path().join("removed GPUI & <数据>");
    let new_dir = temporary.path().join("ElegantClipboard & '数据'");
    fs::create_dir(&new_dir).unwrap();
    let old = entry_path(&base, &label(&old_dir).unwrap());
    let new = entry_path(&base, &label(&new_dir).unwrap());
    let saved = disabled_entry(&executable("previous & version"), &old_dir);
    file::write(&old, &saved).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current & version")).unwrap();
    assert!(!old.exists());
    assert!(!old_dir.exists());
    let migrated = file::read(&new).unwrap().unwrap();
    assert!(
        !plist::parse(&migrated, &label(&new_dir).unwrap(), &new_dir)
            .unwrap()
            .enabled
    );
    assert!(migrated.contains(&plist::arguments(
        executable("current & version").to_str().unwrap(),
        new_dir.to_str().unwrap()
    )));
    migrate_at(&base, &old_dir, &new_dir, &executable("future")).unwrap();
    assert_eq!(file::read(&new).unwrap().unwrap(), migrated);
}

#[test]
fn missing_entry_and_unrelated_launch_agents_remain_untouched() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path().join("LaunchAgents");
    let old_dir = temporary.path().join("old");
    let new_dir = temporary.path().join("new");
    let old = entry_path(&base, &label(&old_dir).unwrap());
    let new = entry_path(&base, &label(&new_dir).unwrap());
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert!(!new.exists());
    let another = entry(&temporary.path().join("another-app"), &old_dir).unwrap();
    file::write(&old, &another).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert_eq!(file::read(&old).unwrap().unwrap(), another);
    assert!(!new.exists());
    let wrong_profile = entry(&executable("old"), &new_dir).unwrap();
    file::write(&old, &wrong_profile).unwrap();
    migrate_at(&base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert_eq!(file::read(&old).unwrap().unwrap(), wrong_profile);
    assert!(!new.exists());
}

#[test]
fn retry_preserves_new_launch_agent_after_user_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let old_dir = base.join("old");
    let new_dir = base.join("new");
    let old = entry_path(base, &label(&old_dir).unwrap());
    let new = entry_path(base, &label(&new_dir).unwrap());
    file::write(&old, &entry(&executable("old"), &old_dir).unwrap()).unwrap();
    let saved = disabled_entry(&executable("user-updated"), &new_dir);
    file::write(&new, &saved).unwrap();
    migrate_at(base, &old_dir, &new_dir, &executable("current")).unwrap();
    assert!(!old.exists());
    assert_eq!(file::read(&new).unwrap().unwrap(), saved);
}

#[test]
fn conflict_and_disk_failure_keep_previous_launch_agent() {
    let temporary = tempfile::tempdir().unwrap();
    let base = temporary.path();
    let old_dir = base.join("old");
    let new_dir = base.join("new");
    let old = entry_path(base, &label(&old_dir).unwrap());
    let new = entry_path(base, &label(&new_dir).unwrap());
    let saved = entry(&executable("old"), &old_dir).unwrap();
    file::write(&old, &saved).unwrap();
    file::write(&new, "<plist/>").unwrap();
    assert!(migrate_at(base, &old_dir, &new_dir, &executable("current")).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
    fs::remove_file(&new).unwrap();
    fs::create_dir(&new).unwrap();
    assert!(migrate_at(base, &old_dir, &new_dir, &executable("current")).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
    fs::remove_dir(&new).unwrap();
    let blocked = base.join("blocked");
    fs::write(&blocked, "file").unwrap();
    assert!(file::migrate(&old, &blocked.join("new.plist"), &saved, &saved).is_err());
    assert_eq!(file::read(&old).unwrap().unwrap(), saved);
}

#[test]
fn duplicate_arguments_or_misleading_nested_keys_are_not_owned_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let data_dir = temporary.path().join("data");
    let label = label(&data_dir).unwrap();
    let content = entry(&executable("old"), &data_dir).unwrap();
    let duplicate = content.replace("</dict>", "<key>ProgramArguments</key><array/></dict>");
    assert!(plist::parse(&duplicate, &label, &data_dir).is_none());
    let nested = content
        .replace(
            "<key>ProgramArguments</key>",
            "<key>Other</key><dict><key>ProgramArguments</key>",
        )
        .replace("</array>", "</array></dict>");
    assert!(plist::parse(&nested, &label, &data_dir).is_none());
}
