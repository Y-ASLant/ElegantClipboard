use super::super::{
    FailureKind,
    file_actions::{explorer_command, item_paths_for_action, plain_text_for_copy, save_item_as},
};
use super::*;
use clipboard_core::preferences::ThemePreference;
use std::path::Path;

#[test]
fn plain_text_paste_failure_is_routed_to_its_pending_item() {
    assert_eq!(
        Command::CopyPlainTextForPaste(42).failure_kind(),
        Some(FailureKind::Paste(42))
    );
    assert_eq!(
        Command::CopyPlainText(42).failure_kind(),
        Some(FailureKind::Other)
    );
}

#[test]
fn plain_text_copy_uses_only_stored_text_and_rejects_missing_representation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let rich = history.capture_rich(Some("<b>格式</b>"), None, Some("格式"), &images)?;
    assert_eq!(plain_text_for_copy(&history.item(rich)?)?, "格式");
    let no_text = history.capture_rich(Some("<i>only html</i>"), None, None, &images)?;
    assert!(plain_text_for_copy(&history.item(no_text)?).is_err());
    let file = history.capture_files(&["C:\\example.txt".into()], &images)?;
    assert!(plain_text_for_copy(&history.item(file)?).is_err());
    Ok(())
}

#[test]
fn file_path_actions_use_resolved_existing_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let source = directory.path().join("中文 file.txt");
    std::fs::write(&source, "content")?;
    let id = history.capture_files(
        &[source.to_string_lossy().into_owned()],
        &directory.path().join("images"),
    )?;
    assert_eq!(
        item_paths_for_action(&history.item(id)?, &directory.path().join("staged"))?,
        vec![source.to_string_lossy().into_owned()]
    );

    let command = explorer_command(&source)?;
    assert!(Path::new(command.get_program()).ends_with("explorer.exe"));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("/select,"), source.as_os_str()]
    );

    std::fs::remove_file(&source)?;
    assert!(item_paths_for_action(&history.item(id)?, &directory.path().join("staged")).is_err());
    let text = history.capture("not a file")?.unwrap();
    assert!(item_paths_for_action(&history.item(text)?, &directory.path().join("staged")).is_err());
    Ok(())
}

#[test]
fn save_as_copies_the_first_resolved_file_and_rejects_unsafe_targets() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let staged = directory.path().join("staged");
    let source = directory.path().join("中文 source.txt");
    std::fs::write(&source, "saved content")?;
    let file_id = history.capture_files(&[source.to_string_lossy().into_owned()], &images)?;
    let destination = directory.path().join("另存 file.txt");
    assert_eq!(
        save_item_as(&history.item(file_id)?, &staged, &destination)?,
        13
    );
    assert_eq!(std::fs::read_to_string(&destination)?, "saved content");
    assert!(save_item_as(&history.item(file_id)?, &staged, &source).is_err());

    let folder = directory.path().join("folder");
    std::fs::create_dir(&folder)?;
    let folder_id = history.capture_files(&[folder.to_string_lossy().into_owned()], &images)?;
    assert!(
        save_item_as(
            &history.item(folder_id)?,
            &staged,
            &directory.path().join("folder-copy")
        )
        .is_err()
    );

    let image_id = history.capture_image(b"\x89PNG\r\n\x1a\nsynthetic", 1, 1, &images)?;
    let image_destination = directory.path().join("image-copy.png");
    save_item_as(&history.item(image_id)?, &staged, &image_destination)?;
    assert_eq!(
        std::fs::read(&image_destination)?,
        b"\x89PNG\r\n\x1a\nsynthetic"
    );
    Ok(())
}

#[test]
fn failed_paste_copy_identifies_only_its_own_request() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::CopyForPaste(424242))?;
    service.send(Command::CopyPathForPaste(424243))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(
        next_event(&events),
        Event::CommandFailed {
            kind: FailureKind::Paste(424242),
            ..
        }
    ));
    assert!(matches!(
        next_event(&events),
        Event::CommandFailed {
            kind: FailureKind::Paste(424243),
            ..
        }
    ));
    assert!(matches!(
        next_event(&events),
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn failed_merge_identifies_only_the_batch_request() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::MergeForPaste(vec![424242, 424243]))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(
        next_event(&events),
        Event::CommandFailed {
            kind: FailureKind::Merge,
            ..
        }
    ));
    assert!(matches!(
        next_event(&events),
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}
