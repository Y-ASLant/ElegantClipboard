use super::*;

#[test]
fn theme_defaults_persists_and_recovers_unknown_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    {
        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.theme()?, ThemePreference::System);
        for theme in [
            ThemePreference::Light,
            ThemePreference::System,
            ThemePreference::Dark,
        ] {
            preferences.set_theme(theme)?;
            assert_eq!(preferences.theme()?, theme);
        }
    }
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.theme()?, ThemePreference::Dark);
    SettingsRepository::new(&db).set(THEME_KEY, "unknown")?;
    assert_eq!(preferences.theme()?, ThemePreference::System);
    Ok(())
}

#[test]
fn hotkey_preference_persists_independently_of_legacy_setting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
    SettingsRepository::new(&db).set("global_shortcut", "Alt+C")?;
    assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
    preferences.set_hotkey(HotkeyPreference::CtrlAltV)?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlAltV);
    preferences.set_hotkey(HotkeyPreference::Disabled)?;
    assert_eq!(preferences.hotkey()?, HotkeyPreference::Disabled);
    SettingsRepository::new(&db).set(HOTKEY_KEY, "unknown")?;
    assert_eq!(preferences.hotkey()?, HotkeyPreference::CtrlShiftV);
    Ok(())
}

#[test]
fn language_defaults_to_chinese_and_persists_english() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.language()?, LanguagePreference::Chinese);
    preferences.set_language(LanguagePreference::English)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.language()?, LanguagePreference::English);
    SettingsRepository::new(&db).set(LANGUAGE_KEY, "unknown")?;
    assert_eq!(preferences.language()?, LanguagePreference::Chinese);
    Ok(())
}

#[test]
fn paused_capture_survives_restart_and_unknown_values_stay_paused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert!(!preferences.capture_paused()?);
    preferences.set_capture_paused(true)?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert!(preferences.capture_paused()?);
    preferences.set_capture_paused(false)?;
    assert!(!preferences.capture_paused()?);
    SettingsRepository::new(&db).set(CAPTURE_PAUSED_KEY, "unknown")?;
    assert!(preferences.capture_paused()?);
    Ok(())
}

#[test]
fn window_size_persists_and_invalid_values_fall_back() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.window_size()?, None);
    let size = WindowSizePreference::new(900, 700).unwrap();
    preferences.set_window_size(size)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.window_size()?, Some(size));
    for invalid in ["419x700", "900x519", "4097x700", "900x2161", "broken"] {
        SettingsRepository::new(&db).set(WINDOW_SIZE_KEY, invalid)?;
        assert_eq!(preferences.window_size()?, None);
    }
    Ok(())
}

#[test]
fn window_behavior_defaults_and_persists_without_restoring_disabled_size() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert!(preferences.persist_window_size()?);
    assert!(!preferences.auto_reset_state()?);
    assert!(!preferences.search_auto_focus()?);
    assert!(preferences.search_auto_clear()?);
    assert!(!preferences.skip_clear_confirm()?);
    assert!(preferences.paste_close_window()?);
    assert_eq!(preferences.paste_key()?, PasteKeyPreference::CtrlV);
    assert!(preferences.paste_move_to_top()?);
    let size = WindowSizePreference::new(900, 700).unwrap();
    preferences.set_window_size(size)?;
    preferences.set_persist_window_size(false)?;
    preferences.set_auto_reset_state(true)?;
    preferences.set_search_auto_focus(true)?;
    preferences.set_search_auto_clear(false)?;
    preferences.set_skip_clear_confirm(true)?;
    preferences.set_paste_close_window(false)?;
    preferences.set_paste_key(PasteKeyPreference::ShiftInsert)?;
    preferences.set_paste_move_to_top(false)?;
    assert_eq!(preferences.window_size()?, None);
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert!(!preferences.persist_window_size()?);
    assert!(preferences.auto_reset_state()?);
    assert!(preferences.search_auto_focus()?);
    assert!(!preferences.search_auto_clear()?);
    assert!(preferences.skip_clear_confirm()?);
    assert!(!preferences.paste_close_window()?);
    assert_eq!(preferences.paste_key()?, PasteKeyPreference::ShiftInsert);
    assert!(!preferences.paste_move_to_top()?);
    preferences.set_persist_window_size(true)?;
    assert_eq!(preferences.window_size()?, None);
    preferences.set_window_size(size)?;
    assert_eq!(preferences.window_size()?, Some(size));
    Ok(())
}

#[test]
fn quick_paste_is_opt_in_and_explicit_choice_survives_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert!(!preferences.quick_paste_enabled()?);
    preferences.set_quick_paste_enabled(true)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert!(preferences.quick_paste_enabled()?);
    preferences.set_quick_paste_enabled(false)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    assert!(!Preferences::new(&db).quick_paste_enabled()?);
    Ok(())
}

#[test]
fn paste_shortcut_slots_persist_and_invalid_stored_json_uses_defaults() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    let mut shortcuts = preferences.paste_shortcuts()?;
    assert_eq!(shortcuts.slot(false, 10), Some("Alt+0"));
    assert_eq!(shortcuts.slot(true, 4), Some(""));
    assert_eq!(shortcuts.slot(true, 11), None);
    shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
    shortcuts.set_slot(false, 1, String::new())?;
    assert!(shortcuts.set_slot(true, 0, String::new()).is_err());
    preferences.set_paste_shortcuts(&shortcuts)?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.paste_shortcuts()?, shortcuts);
    SettingsRepository::new(&db).set(PASTE_SHORTCUTS_KEY, "invalid")?;
    assert_eq!(
        preferences.paste_shortcuts()?,
        PasteShortcutConfig::default()
    );
    Ok(())
}

#[test]
fn window_position_persists_and_unknown_values_follow_cursor() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.window_position()?,
        WindowPositionPreference::FollowCursor
    );
    preferences.set_window_position(WindowPositionPreference::ScreenCenter)?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.window_position()?,
        WindowPositionPreference::ScreenCenter
    );
    preferences.set_window_position(WindowPositionPreference::FixedPosition)?;
    assert_eq!(
        preferences.window_position()?,
        WindowPositionPreference::FixedPosition
    );
    SettingsRepository::new(&db).set(WINDOW_POSITION_KEY, "unknown")?;
    assert_eq!(
        preferences.window_position()?,
        WindowPositionPreference::FollowCursor
    );
    Ok(())
}

#[test]
fn hover_preview_defaults_persist_and_reject_invalid_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.hover_preview()?,
        HoverPreviewPreference::default()
    );
    assert!(preferences.hover_preview()?.allows("image"));
    assert!(preferences.hover_preview()?.allows("text"));
    assert!(preferences.hover_preview()?.allows("url"));
    assert!(preferences.hover_preview()?.allows("html"));
    assert!(preferences.hover_preview()?.allows("rtf"));
    assert!(!preferences.hover_preview()?.allows("files"));
    let preference = HoverPreviewPreference {
        text: false,
        files: true,
        expanded_image: true,
        delay_ms: 750,
        position: HoverPreviewPosition::Left,
        zoom_step: 20,
        ..HoverPreviewPreference::default()
    };
    preferences.set_hover_preview(preference)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.hover_preview()?, preference);
    assert!(!preferences.hover_preview()?.allows("rtf"));
    assert!(
        preferences
            .set_hover_preview(HoverPreviewPreference {
                delay_ms: 99,
                ..preference
            })
            .is_err()
    );
    assert_eq!(preferences.hover_preview()?, preference);
    SettingsRepository::new(&db).set(HOVER_PREVIEW_KEY, "invalid")?;
    assert_eq!(
        preferences.hover_preview()?,
        HoverPreviewPreference::default()
    );
    Ok(())
}

#[test]
fn display_preferences_survive_restart_and_recover_invalid_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.display()?, DisplayPreference::default());
    let display = DisplayPreference {
        show_category_filter: false,
        show_drag_area_indicator: false,
        card_density: CardDensity::Compact,
        card_max_lines: 5,
        show_time: false,
        time_format: TimeFormat::Relative,
        show_char_count: false,
        show_byte_size: false,
        show_source_app: false,
        source_app_display: SourceAppDisplay::Icon,
    };
    preferences.set_display(display)?;
    assert!(
        preferences
            .set_display(DisplayPreference {
                card_max_lines: 11,
                ..display
            })
            .is_err()
    );
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.display()?, display);
    SettingsRepository::new(&db).set(DISPLAY_KEY, "invalid")?;
    assert_eq!(preferences.display()?, DisplayPreference::default());
    SettingsRepository::new(&db).set(
        DISPLAY_KEY,
        r#"{"show_category_filter":false,"card_density":"compact"}"#,
    )?;
    assert_eq!(
        preferences.display()?,
        DisplayPreference {
            show_category_filter: false,
            card_density: CardDensity::Compact,
            ..DisplayPreference::default()
        }
    );
    Ok(())
}

#[test]
fn audio_preferences_survive_restart_and_recover_invalid_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.audio()?, AudioPreference::default());
    let audio = AudioPreference {
        copy_enabled: true,
        copy_timing: SoundTiming::AfterSuccess,
        paste_enabled: true,
        paste_timing: SoundTiming::Immediate,
    };
    preferences.set_audio(audio)?;
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.audio()?, audio);
    SettingsRepository::new(&db).set(AUDIO_KEY, "invalid")?;
    assert_eq!(preferences.audio()?, AudioPreference::default());
    Ok(())
}

#[test]
fn monitor_types_survive_restart_and_reject_disabling_everything() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.monitor_types()?,
        MonitorTypesPreference::default()
    );
    let types = MonitorTypesPreference {
        text: false,
        url: true,
        html: false,
        rtf: false,
        image: false,
        files: false,
    };
    preferences.set_monitor_types(types)?;
    assert!(
        preferences
            .set_monitor_types(MonitorTypesPreference {
                url: false,
                ..types
            })
            .is_err()
    );
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.monitor_types()?, types);
    SettingsRepository::new(&db).set(MONITOR_TYPES_KEY, "broken")?;
    assert_eq!(
        preferences.monitor_types()?,
        MonitorTypesPreference::default()
    );
    Ok(())
}

#[test]
fn app_filter_matches_names_processes_paths_and_wildcards() {
    let blacklist = AppFilterPreference::default()
        .with_rule("notepad.exe")
        .unwrap()
        .with_rule("*\\Private\\*")
        .unwrap();
    let blacklist = AppFilterPreference {
        enabled: true,
        ..blacklist
    };
    assert!(blacklist.excludes(Some("Notepad"), Some("C:\\Windows\\notepad.exe")));
    assert!(blacklist.excludes(Some("Other"), Some("C:\\Private\\Editor.exe")));
    assert!(!blacklist.excludes(Some("Browser"), Some("C:\\Apps\\browser.exe")));
    assert!(!blacklist.excludes(None, None));
    let whitelist = AppFilterPreference {
        mode: AppFilterMode::Whitelist,
        ..blacklist
    };
    assert!(!whitelist.excludes(Some("Notepad"), Some("C:\\Windows\\notepad.exe")));
    assert!(whitelist.excludes(Some("Browser"), Some("C:\\Apps\\browser.exe")));
    assert!(!whitelist.excludes(None, None));
    assert!(
        !AppFilterPreference {
            rules: vec![" ".into()],
            ..whitelist
        }
        .valid()
    );
    assert_eq!(
        whitelist.clone().with_rule("NOTEPAD.EXE").unwrap(),
        whitelist
    );
}

#[test]
fn app_filter_persists_and_recovers_invalid_json() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    let filter = AppFilterPreference {
        enabled: true,
        mode: AppFilterMode::Whitelist,
        rules: vec!["Code.exe".into(), "*\\Work\\*".into()],
    };
    preferences.set_app_filter(&filter)?;
    assert!(
        preferences
            .set_app_filter(&AppFilterPreference {
                rules: vec!["bad\nrule".into()],
                ..filter.clone()
            })
            .is_err()
    );
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.app_filter()?, filter);
    SettingsRepository::new(&db).set(APP_FILTER_KEY, "broken")?;
    assert_eq!(preferences.app_filter()?, AppFilterPreference::default());
    Ok(())
}

#[test]
fn onboarding_completion_is_saved_in_gpui_settings() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert!(!preferences.onboarding_completed()?);
    preferences.set_onboarding_completed()?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    assert!(Preferences::new(&db).onboarding_completed()?);
    Ok(())
}
