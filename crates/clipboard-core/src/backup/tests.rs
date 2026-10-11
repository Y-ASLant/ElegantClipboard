use super::*;
use crate::{
    History, PreviewContent,
    preferences::{
        AppFilterMode, AppFilterPreference, AudioPreference, CardDensity, DisplayPreference,
        HoverPreviewPreference, LanguagePreference, MonitorTypesPreference, PasteKeyPreference,
        PasteShortcutConfig, Preferences, SoundTiming, SourceAppDisplay, ThemePreference,
        TimeFormat, WindowPositionPreference, WindowSizePreference,
    },
};

#[test]
fn live_backup_restores_text_groups_images_and_gpui_settings() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source");
    fs::create_dir_all(&source)?;
    let path = source.join("clipboard.db");
    let history = History::open(path)?;
    let id = history.capture("backup text")?.unwrap();
    history.toggle_favorite(id)?;
    let group = history.create_group("工作")?;
    history.move_to_group(id, None, Some(group.id))?;
    let png = b"\x89PNG\r\n\x1a\nsynthetic";
    let image = history.capture_image(png, 3, 2, &source.join("images"))?;
    let icon = source.join("icons/app.ico");
    fs::create_dir_all(icon.parent().unwrap())?;
    fs::write(&icon, b"icon bytes")?;
    let staged = source.join("staged/draft.txt");
    fs::create_dir_all(staged.parent().unwrap())?;
    fs::write(&staged, b"draft bytes")?;
    let original_file = "Z:\\ElegantClipboard-QA-missing\\draft.txt".to_owned();
    let file =
        history.capture_files(std::slice::from_ref(&original_file), &source.join("images"))?;
    let payload = serde_json::json!({
        "staged": [{"original": original_file, "staged": staged.to_string_lossy()}]
    });
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET source_app_icon = ?1 WHERE id = ?2",
        params![icon.to_string_lossy().as_ref(), id],
    )?;
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = ?2",
        params![payload.to_string(), file],
    )?;
    let preferences = Preferences::new(&history.db);
    preferences.set_theme(ThemePreference::Dark)?;
    preferences.set_language(LanguagePreference::English)?;
    let window_size = WindowSizePreference::new(900, 700).unwrap();
    preferences.set_window_size(window_size)?;
    preferences.set_persist_window_size(true)?;
    preferences.set_auto_reset_state(true)?;
    preferences.set_search_auto_focus(true)?;
    preferences.set_search_auto_clear(false)?;
    preferences.set_paste_close_window(false)?;
    preferences.set_paste_key(PasteKeyPreference::ShiftInsert)?;
    preferences.set_paste_move_to_top(false)?;
    preferences.set_quick_paste_enabled(false)?;
    let mut paste_shortcuts = PasteShortcutConfig::default();
    paste_shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
    preferences.set_paste_shortcuts(&paste_shortcuts)?;
    preferences.set_window_position(WindowPositionPreference::ScreenCenter)?;
    let hover_preference = HoverPreviewPreference {
        text: true,
        expanded_image: true,
        delay_ms: 750,
        ..HoverPreviewPreference::default()
    };
    preferences.set_hover_preview(hover_preference)?;
    let display = DisplayPreference {
        show_category_filter: false,
        show_drag_area_indicator: false,
        card_density: CardDensity::Spacious,
        card_max_lines: 5,
        show_time: false,
        time_format: TimeFormat::Relative,
        show_char_count: false,
        show_byte_size: false,
        show_source_app: false,
        source_app_display: SourceAppDisplay::Name,
    };
    preferences.set_display(display)?;
    let audio = AudioPreference {
        copy_enabled: true,
        copy_timing: SoundTiming::AfterSuccess,
        ..AudioPreference::default()
    };
    preferences.set_audio(audio)?;
    let monitor_types = MonitorTypesPreference {
        image: false,
        ..MonitorTypesPreference::default()
    };
    preferences.set_monitor_types(monitor_types)?;
    let app_filter = AppFilterPreference {
        enabled: true,
        mode: AppFilterMode::Blacklist,
        rules: vec!["*browser*".into()],
    };
    preferences.set_app_filter(&app_filter)?;
    preferences.set_onboarding_completed()?;
    history.db.write_connection().lock().execute(
        "INSERT INTO settings (key, value) VALUES ('secret_token', 'do-not-export')",
        [],
    )?;
    history.db.write_connection().lock().execute(
        "INSERT INTO settings (key, value) VALUES ('gpui_toolbar', 'hidden-batch')",
        [],
    )?;
    let backup = dir.path().join("history.zip");
    let exported = history.export_backup(&backup)?;
    assert_eq!(exported.total_items, 3);
    assert_eq!(exported.included_images, 1);
    assert_eq!(exported.missing_images, 0);
    assert_eq!((exported.included_icons, exported.included_staged), (1, 1));
    assert!(history.export_backup(&backup).is_err());

    let target = dir.path().join("restored");
    fs::create_dir_all(&target)?;
    let report = restore_backup(&backup, &target)?;
    assert_eq!(report.total_items, 3);
    assert_eq!(report.restored_images, 1);
    assert_eq!((report.restored_icons, report.restored_staged), (1, 1));
    let restored = History::open(target.join("clipboard.db"))?;
    assert_eq!(restored.groups()?[0].name, "工作");
    assert!(restored.item(id)?.is_favorite);
    assert_eq!(restored.text(id)?, "backup text");
    assert_eq!(
        Preferences::new(&restored.db).theme()?,
        ThemePreference::Dark
    );
    assert_eq!(
        Preferences::new(&restored.db).language()?,
        LanguagePreference::English
    );
    assert_eq!(
        Preferences::new(&restored.db).window_size()?,
        Some(window_size)
    );
    assert!(Preferences::new(&restored.db).persist_window_size()?);
    assert!(Preferences::new(&restored.db).auto_reset_state()?);
    assert!(Preferences::new(&restored.db).search_auto_focus()?);
    assert!(!Preferences::new(&restored.db).search_auto_clear()?);
    assert!(!Preferences::new(&restored.db).paste_close_window()?);
    assert_eq!(
        Preferences::new(&restored.db).paste_key()?,
        PasteKeyPreference::ShiftInsert
    );
    assert!(!Preferences::new(&restored.db).paste_move_to_top()?);
    assert!(!Preferences::new(&restored.db).quick_paste_enabled()?);
    assert_eq!(
        Preferences::new(&restored.db).paste_shortcuts()?,
        paste_shortcuts
    );
    assert_eq!(
        Preferences::new(&restored.db).window_position()?,
        WindowPositionPreference::ScreenCenter
    );
    assert_eq!(
        Preferences::new(&restored.db).hover_preview()?,
        hover_preference
    );
    assert_eq!(Preferences::new(&restored.db).display()?, display);
    assert_eq!(Preferences::new(&restored.db).audio()?, audio);
    assert_eq!(
        Preferences::new(&restored.db).monitor_types()?,
        monitor_types
    );
    assert_eq!(Preferences::new(&restored.db).app_filter()?, app_filter);
    assert!(Preferences::new(&restored.db).onboarding_completed()?);
    assert!(
        matches!(restored.preview_content(image)?, PreviewContent::Image(path) if path.starts_with(target.join("images")) && fs::read(&path)? == png)
    );
    let icon = PathBuf::from(restored.item(id)?.source_app_icon.unwrap());
    assert!(icon.starts_with(target.join("icons")));
    assert_eq!(fs::read(icon)?, b"icon bytes");
    let copied = restored.files_for_copy(file, &target.join("staged"))?;
    assert_eq!(copied.len(), 1);
    assert_eq!(fs::read(&copied[0])?, b"draft bytes");
    assert_eq!(
        restored.db.read_connection().lock().query_row(
            "SELECT COUNT(*) FROM settings WHERE key IN ('secret_token', 'gpui_toolbar')",
            [],
            |row| row.get::<_, i64>(0),
        )?,
        0
    );
    assert!(restore_backup(&backup, &target).is_err());
    Ok(())
}

#[test]
fn edited_url_stays_text_after_gpui_backup_restore() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("source.db"))?;
    let id = history.capture("https://example.com/original")?.unwrap();
    let item = history.item(id)?;
    assert_eq!(item.content_type, "url");
    history.edit_text(
        id,
        &item.content_hash,
        "https://example.com/edited",
        dir.path(),
    )?;
    assert_eq!(history.item(id)?.content_type, "text");

    let backup = dir.path().join("history.zip");
    history.export_backup(&backup)?;
    let target = dir.path().join("restored");
    fs::create_dir(&target)?;
    restore_backup(&backup, &target)?;
    let restored = History::open(target.join("clipboard.db"))?;
    assert_eq!(restored.item(id)?.content_type, "text");
    assert_eq!(restored.text(id)?, "https://example.com/edited");
    Ok(())
}

#[test]
fn restores_previous_gpui_v1_backup() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source_path = dir.path().join("source.db");
    let history = History::open(source_path.clone())?;
    let id = history.capture("v1 text")?.unwrap();
    let legacy_url_id = history.capture("https://example.com/v1")?.unwrap();
    history.db.write_connection().lock().execute(
        "UPDATE clipboard_items SET content_type = 'text' WHERE id = ?1",
        params![legacy_url_id],
    )?;
    history.db.write_connection().lock().execute(
        "DELETE FROM settings WHERE key = '_migration_url_content_type'",
        [],
    )?;
    history.db.write_connection().lock().execute(
        "INSERT INTO settings (key, value) VALUES ('secret_token', 'legacy-secret')",
        [],
    )?;
    drop(history);
    let source_db = Connection::open(&source_path)?;
    source_db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    let source_secret_count: i64 = source_db.query_row(
        "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(source_secret_count, 1);
    drop(source_db);
    let previous = dir.path().join("previous.zip");
    let mut output = ZipWriter::new(File::create(&previous)?);
    output.start_file("manifest.json", SimpleFileOptions::default())?;
    output.write_all(
        br#"{"format":"elegantclipboard-gpui","version":1,"total_items":2,"missing_images":0}"#,
    )?;
    output.start_file("clipboard.db", SimpleFileOptions::default())?;
    io::copy(&mut File::open(&source_path)?, &mut output)?;
    output.finish()?;
    let target = dir.path().join("restored");
    fs::create_dir_all(&target)?;
    let report = restore_backup(&previous, &target)?;
    assert_eq!(report.total_items, 2);
    assert_eq!((report.restored_icons, report.restored_staged), (0, 0));
    assert_eq!(
        History::open(target.join("clipboard.db"))?.text(id)?,
        "v1 text"
    );
    assert_eq!(
        History::open(target.join("clipboard.db"))?
            .item(legacy_url_id)?
            .content_type,
        "url"
    );
    let restored_db = Connection::open(target.join("clipboard.db"))?;
    let secret_count: i64 = restored_db.query_row(
        "SELECT COUNT(*) FROM settings WHERE key = 'secret_token'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(secret_count, 0);
    Ok(())
}

#[test]
fn invalid_backup_cannot_create_destination_database() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let invalid = dir.path().join("invalid.zip");
    fs::write(&invalid, b"not a zip")?;
    let target = dir.path().join("target");
    fs::create_dir_all(&target)?;
    assert!(restore_backup(&invalid, &target).is_err());
    assert!(!target.join("clipboard.db").exists());
    Ok(())
}

#[test]
fn archive_path_traversal_is_rejected_before_installation() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = History::open(dir.path().join("source.db"))?;
    source.capture("safe text")?;
    let valid = dir.path().join("valid.zip");
    source.export_backup(&valid)?;
    let malicious = dir.path().join("malicious.zip");
    let mut input = ZipArchive::new(File::open(valid)?)?;
    let mut output = ZipWriter::new(File::create(&malicious)?);
    for index in 0..input.len() {
        let mut entry = input.by_index(index)?;
        output.start_file(entry.name(), SimpleFileOptions::default())?;
        io::copy(&mut entry, &mut output)?;
    }
    output.start_file("images/../outside.txt", SimpleFileOptions::default())?;
    output.write_all(b"outside")?;
    output.finish()?;
    let target = dir.path().join("restored");
    fs::create_dir_all(&target)?;
    assert!(restore_backup(&malicious, &target).is_err());
    assert!(!target.join("clipboard.db").exists());
    assert!(!target.join("outside.txt").exists());
    Ok(())
}

#[test]
fn duplicate_entries_are_rejected_without_creating_restore_files() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let source = History::open(dir.path().join("source.db"))?;
    source.capture("safe text")?;
    let valid = dir.path().join("valid.zip");
    source.export_backup(&valid)?;

    for (index, duplicate) in ["manifest.json", "clipboard.db", "images/extra.png"]
        .into_iter()
        .enumerate()
    {
        let archive_path = dir.path().join(format!("duplicate-{index}.zip"));
        let mut input = ZipArchive::new(File::open(&valid)?)?;
        let mut output = ZipWriter::new(File::create(&archive_path)?);
        for index in 0..input.len() {
            let mut entry = input.by_index(index)?;
            output.start_file(entry.name(), SimpleFileOptions::default())?;
            io::copy(&mut entry, &mut output)?;
        }
        if duplicate.starts_with("images/") {
            output.start_file(duplicate, SimpleFileOptions::default())?;
            output.write_all(b"first")?;
        }
        // ZipWriter refuses duplicate names; replace an equal-length placeholder
        // in both ZIP headers to exercise a duplicate produced elsewhere.
        let placeholder = "Q".repeat(duplicate.len());
        output.start_file(&placeholder, SimpleFileOptions::default())?;
        output.write_all(b"second")?;
        output.finish()?;
        let mut bytes = fs::read(&archive_path)?;
        let occurrences: Vec<_> = bytes
            .windows(placeholder.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == placeholder.as_bytes()).then_some(offset))
            .collect();
        assert_eq!(occurrences.len(), 2);
        for offset in occurrences {
            bytes[offset..offset + duplicate.len()].copy_from_slice(duplicate.as_bytes());
        }
        fs::write(&archive_path, bytes)?;
        ZipArchive::new(File::open(&archive_path)?)?;
        let target = dir.path().join(format!("target-{index}"));
        fs::create_dir_all(&target)?;
        assert!(restore_backup(&archive_path, &target).is_err());
        assert_eq!(fs::read_dir(&target)?.count(), 0);
    }
    Ok(())
}

#[test]
fn archive_limits_are_checked_before_extraction() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("source.db"))?;
    history.capture("safe text")?;
    let backup = dir.path().join("valid.zip");
    history.export_backup(&backup)?;
    let source = File::open(&backup)?;
    let mut raw_source = source.try_clone()?;
    let mut archive = ZipArchive::new(source)?;
    assert!(preflight_archive(&mut archive, &mut raw_source, 1, u64::MAX).is_err());
    assert!(preflight_archive(&mut archive, &mut raw_source, MAX_ENTRIES, 1).is_err());
    preflight_archive(
        &mut archive,
        &mut raw_source,
        MAX_ENTRIES,
        MAX_ARCHIVE_BYTES,
    )?;
    Ok(())
}
