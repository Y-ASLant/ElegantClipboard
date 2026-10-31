use super::*;
use crate::{preferences::*, retention::RetentionPolicy};
use std::fs;
use zip::ZipArchive;

#[test]
fn backup_roundtrips_all_non_default_preferences() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("source.db"))?;
    let preferences = Preferences::new(&history.db);
    let mut expected = PreferenceSnapshot {
        theme: ThemePreference::Dark,
        theme_color: ThemeColor::VibrantOrange,
        corner_radius: CornerRadiusPreference::Fixed(12),
        language: LanguagePreference::English,
        hotkey: HotkeyPreference::Shortcut("Ctrl+Shift+F8".into()),
        paused: true,
        window_size: Some(WindowSizePreference::new(900, 700).unwrap()),
        persist_window_size: true,
        auto_reset_state: true,
        search_auto_focus: true,
        search_auto_clear: false,
        skip_clear_confirm: true,
        paste_close_window: false,
        paste_key: PasteKeyPreference::ShiftInsert,
        paste_move_to_top: false,
        dedup_strategy: DedupStrategy::AlwaysNew,
        restore_clipboard: true,
        sequence_hotkey: "Ctrl+Alt+F9".into(),
        quick_paste_enabled: true,
        paste_shortcuts: PasteShortcutConfig {
            recent: std::array::from_fn(|index| format!("Ctrl+Shift+F{}", index + 1)),
            favorites: std::array::from_fn(|index| format!("Ctrl+Alt+F{}", index + 1)),
        },
        window_position: WindowPositionPreference::ScreenCenter,
        hover_preview: HoverPreviewPreference {
            image: false,
            text: false,
            files: true,
            expanded_image: true,
            delay_ms: 750,
            position: HoverPreviewPosition::Left,
            zoom_step: 25,
            ..HoverPreviewPreference::default()
        },
        display: DisplayPreference {
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
        },
        audio: AudioPreference {
            copy_enabled: true,
            copy_timing: SoundTiming::AfterSuccess,
            paste_enabled: true,
            paste_timing: SoundTiming::AfterSuccess,
        },
        retention: RetentionPolicy {
            days: 30,
            max_count: 500,
        },
        retention_saved: true,
        transient_filter: false,
        monitor_types: MonitorTypesPreference {
            text: true,
            url: false,
            html: false,
            rtf: false,
            image: false,
            files: false,
        },
        app_filter: AppFilterPreference {
            enabled: true,
            mode: AppFilterMode::Whitelist,
            rules: vec!["*editor*".into()],
        },
        window_effect: WindowEffectPreference {
            effect: WindowEffect::Frosted,
            opacity: 45,
        },
        onboarding_completed: true,
    };
    preferences.set_theme(expected.theme)?;
    preferences.set_theme_color(expected.theme_color)?;
    preferences.set_corner_radius(expected.corner_radius)?;
    preferences.set_language(expected.language)?;
    preferences.set_hotkey(expected.hotkey.clone())?;
    preferences.set_capture_paused(expected.paused)?;
    preferences.set_window_size(expected.window_size.unwrap())?;
    preferences.set_persist_window_size(expected.persist_window_size)?;
    preferences.set_auto_reset_state(expected.auto_reset_state)?;
    preferences.set_search_auto_focus(expected.search_auto_focus)?;
    preferences.set_search_auto_clear(expected.search_auto_clear)?;
    preferences.set_skip_clear_confirm(expected.skip_clear_confirm)?;
    preferences.set_paste_close_window(expected.paste_close_window)?;
    preferences.set_paste_key(expected.paste_key)?;
    preferences.set_paste_move_to_top(expected.paste_move_to_top)?;
    preferences.set_dedup_strategy(expected.dedup_strategy)?;
    preferences.set_restore_clipboard(expected.restore_clipboard)?;
    preferences.set_sequence_paste_hotkey(&expected.sequence_hotkey)?;
    preferences.set_quick_paste_enabled(expected.quick_paste_enabled)?;
    preferences.set_paste_shortcuts(&expected.paste_shortcuts)?;
    preferences.set_window_position(expected.window_position)?;
    preferences.set_hover_preview(expected.hover_preview)?;
    preferences.set_display(expected.display)?;
    preferences.set_audio(expected.audio)?;
    history.save_retention(expected.retention, true, &dir.path().join("images"))?;
    preferences.set_transient_filter(expected.transient_filter)?;
    preferences.set_monitor_types(expected.monitor_types)?;
    preferences.set_app_filter(&expected.app_filter)?;
    preferences.set_window_effect(expected.window_effect)?;
    preferences.set_onboarding_completed()?;

    // Both a saved size and disabling size persistence must survive a backup.
    for persist_window_size in [true, false] {
        preferences.set_persist_window_size(persist_window_size)?;
        if !persist_window_size {
            expected.persist_window_size = false;
            expected.window_size = None;
        }
        let backup = dir
            .path()
            .join(format!("preferences-{persist_window_size}.zip"));
        history.export_backup(&backup)?;
        let target = dir.path().join(format!("restored-{persist_window_size}"));
        fs::create_dir(&target)?;
        restore_backup(&backup, &target)?;
        let restored = History::open(target.join("clipboard.db"))?;
        assert_eq!(Preferences::new(&restored.db).snapshot()?, expected);
    }
    Ok(())
}

#[test]
fn exported_settings_exclude_unknown_and_obsolete_keys_without_mutating_source() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("source.db"))?;
    history.db.write_connection().lock().execute_batch(
        "INSERT INTO settings (key, value) VALUES
         ('secret_token', 'private'), ('gpui_toolbar', 'obsolete'),
         ('gpui_hostile_unknown', 'untrusted')",
    )?;
    let backup = dir.path().join("preferences.zip");
    history.export_backup(&backup)?;
    let mut archive = ZipArchive::new(File::open(backup)?)?;
    let snapshot_path = dir.path().join("snapshot.db");
    io::copy(
        &mut archive.by_name("clipboard.db")?,
        &mut File::create(&snapshot_path)?,
    )?;
    let snapshot = Connection::open(snapshot_path)?;
    let unknown_count = |connection: &Connection| -> rusqlite::Result<i64> {
        connection.query_row(
            "SELECT COUNT(*) FROM settings
             WHERE key IN ('secret_token', 'gpui_toolbar', 'gpui_hostile_unknown')",
            [],
            |row| row.get(0),
        )
    };
    assert_eq!(unknown_count(&snapshot)?, 0);
    assert_eq!(unknown_count(&history.db.read_connection().lock())?, 3);
    let migration_count: i64 = snapshot.query_row(
        "SELECT COUNT(*) FROM settings WHERE value = 'done'
         AND key IN ('_migration_url_content_type', '_migration_backfill_semantic_hash')",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(migration_count, 2);
    Ok(())
}
