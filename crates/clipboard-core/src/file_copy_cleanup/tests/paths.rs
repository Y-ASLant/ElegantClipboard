use super::*;

#[cfg(windows)]
fn link_directory(source: &Path, destination: &Path) -> Result<()> {
    // junction 无需符号链接权限，可在 CI 中实际验证重解析点隔离。
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(destination)
        .arg(source)
        .output()?;
    ensure!(
        output.status.success(),
        "junction fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(unix)]
fn link_directory(source: &Path, destination: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, destination)?;
    Ok(())
}

#[test]
fn nested_links_are_removed_without_following_external_directories() -> Result<()> {
    let dir = isolated_directory()?;
    let profile = dir.path().join("profile");
    let external = dir.path().join("external");
    fs::create_dir(&external)?;
    fs::write(external.join("keep.txt"), b"external bytes")?;
    let history = History::open(profile.join("clipboard.db"))?;
    fs::create_dir_all(profile.join("staged/old-hash"))?;
    link_directory(&external, &profile.join("staged/old-hash/link"))?;
    fs::write(profile.join("staged/ordinary.txt"), b"remove copy")?;
    history.remove_file_copies(&profile)?;
    assert!(!profile.join("staged").exists());
    assert_eq!(fs::read(external.join("keep.txt"))?, b"external bytes");
    Ok(())
}

#[test]
fn staged_root_link_is_rejected_then_cleanup_retries_after_root_is_repaired() -> Result<()> {
    let dir = isolated_directory()?;
    let profile = dir.path().join("profile");
    let external = dir.path().join("external");
    fs::create_dir(&external)?;
    fs::write(external.join("keep.txt"), b"external bytes")?;
    let history = History::open(profile.join("clipboard.db"))?;
    let id = history.capture("legacy")?.unwrap();
    set_payload(
        &history,
        id,
        r#"{"staged":[{"staged":"old"}],"legacy":true}"#,
    )?;
    link_directory(&external, &profile.join("staged"))?;
    assert!(history.remove_file_copies(&profile).is_err());
    assert_eq!(
        history.item(id)?.file_payload.as_deref(),
        Some(r#"{"legacy":true}"#)
    );
    assert_eq!(fs::read(external.join("keep.txt"))?, b"external bytes");
    #[cfg(windows)]
    fs::remove_dir(profile.join("staged"))?;
    #[cfg(unix)]
    fs::remove_file(profile.join("staged"))?;
    fs::create_dir(profile.join("staged"))?;
    fs::write(profile.join("staged/copy"), b"retry copy")?;
    history.remove_file_copies(&profile)?;
    assert!(!profile.join("staged").exists());
    assert_eq!(fs::read(external.join("keep.txt"))?, b"external bytes");
    Ok(())
}

#[test]
fn linked_data_directory_is_not_used_to_delete_real_profile_copies() -> Result<()> {
    let dir = isolated_directory()?;
    let profile = dir.path().join("profile");
    let alias = dir.path().join("linked-profile");
    let history = History::open(profile.join("clipboard.db"))?;
    fs::create_dir(profile.join("staged"))?;
    fs::write(profile.join("staged/copy"), b"owned copy")?;
    link_directory(&profile, &alias)?;
    assert!(history.remove_file_copies(&alias).is_err());
    assert_eq!(fs::read(profile.join("staged/copy"))?, b"owned copy");
    history.remove_file_copies(&profile)?;
    assert!(!profile.join("staged").exists());
    #[cfg(windows)]
    fs::remove_dir(alias)?;
    #[cfg(unix)]
    fs::remove_file(alias)?;
    Ok(())
}

#[cfg(windows)]
#[test]
fn locked_copy_failure_preserves_pending_file_and_retries_after_reopen() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = isolated_directory()?;
    let db_path = dir.path().join("clipboard.db");
    let history = History::open(db_path.clone())?;
    let id = history.capture("legacy")?.unwrap();
    set_payload(&history, id, r#"{"staged":[{"staged":"ignored"}]}"#)?;
    fs::create_dir_all(dir.path().join("staged/hash/nested"))?;
    let copy = dir.path().join("staged/hash/nested/copy");
    fs::write(&copy, b"locked copy")?;
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&copy)?;
    assert!(history.remove_file_copies(dir.path()).is_err());
    assert_eq!(history.item(id)?.file_payload.as_deref(), Some("{}"));
    assert_eq!(fs::read(&copy)?, b"locked copy");
    drop(locked);
    drop(history);
    let reopened = History::open(db_path)?;
    reopened.remove_file_copies(dir.path())?;
    assert!(!dir.path().join("staged").exists());
    assert_eq!(reopened.text(id)?, "legacy");
    Ok(())
}
