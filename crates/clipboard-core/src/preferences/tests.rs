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
    assert_eq!(preferences.hotkey()?, HotkeyPreference::default());
    SettingsRepository::new(&db).set("global_shortcut", "Alt+C")?;
    assert_eq!(preferences.hotkey()?, HotkeyPreference::default());
    preferences.set_hotkey(HotkeyPreference::Shortcut("Ctrl+Alt+F9".into()))?;
    drop(preferences);
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.hotkey()?,
        HotkeyPreference::Shortcut("Ctrl+Alt+F9".into())
    );
    preferences.set_hotkey(HotkeyPreference::Disabled)?;
    assert_eq!(preferences.hotkey()?, HotkeyPreference::Disabled);
    for (stored, expected) in [
        ("ctrl_shift_v", HotkeyPreference::default()),
        ("alt_c", HotkeyPreference::Shortcut("Alt+C".into())),
        (
            "ctrl_alt_v",
            HotkeyPreference::Shortcut("Ctrl+Alt+V".into()),
        ),
        ("disabled", HotkeyPreference::Disabled),
        ("unknown", HotkeyPreference::default()),
    ] {
        SettingsRepository::new(&db).set(HOTKEY_KEY, stored)?;
        assert_eq!(preferences.hotkey()?, expected);
    }
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
