use super::*;

#[test]
fn hover_preview_only_allows_known_types_enabled_by_their_category() {
    for image in [false, true] {
        for text in [false, true] {
            for files in [false, true] {
                let preference = HoverPreviewPreference {
                    image,
                    text,
                    files,
                    ..HoverPreviewPreference::default()
                };
                for (kind, allowed) in [
                    ("image", image),
                    ("files", files),
                    ("text", text),
                    ("url", text),
                    ("html", text),
                    ("rtf", text),
                    ("future-format", false),
                    ("", false),
                    ("Text", false),
                ] {
                    assert_eq!(preference.allows(kind), allowed, "{kind}");
                }
            }
        }
    }
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
fn saved_hover_preview_without_detail_flags_stays_fully_visible() {
    let parsed: HoverPreviewPreference = serde_json::from_str(
        r#"{"image":true,"text":true,"files":false,"expanded_image":false,"delay_ms":500,"position":"auto","zoom_step":10}"#,
    )
    .unwrap();
    assert_eq!(parsed.details, HoverPreviewDetails::default());
    assert!(parsed.details.formats && parsed.details.path && parsed.details.hint);
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
        category_tabs: CategoryTabVisibility {
            all: true,
            favorites: false,
            text: true,
            image: true,
            url: false,
            other: false,
        },
        show_drag_area_indicator: false,
        card_density: CardDensity::Compact,
        card_max_lines: 5,
        show_time: false,
        time_format: TimeFormat::Relative,
        show_char_count: false,
        show_byte_size: false,
        show_source_app: false,
        source_app_display: SourceAppDisplay::Icon,
        show_card_number: false,
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
fn missing_category_tabs_use_all_favorites_and_other() {
    assert_eq!(
        CategoryTabVisibility::default(),
        CategoryTabVisibility {
            all: true,
            favorites: true,
            text: false,
            image: false,
            url: false,
            other: true,
        }
    );
    let parsed: CategoryTabVisibility =
        serde_json::from_str(r#"{"all":true,"favorites":false,"text":true,"other":true}"#).unwrap();
    assert!(parsed.text && parsed.other);
    assert!(!parsed.favorites && !parsed.image && !parsed.url);
}

#[test]
fn display_requires_all_tab_and_at_least_two_category_tabs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let preferences = Preferences::new(&db);
    let none = DisplayPreference {
        show_category_filter: true,
        category_tabs: CategoryTabVisibility {
            all: false,
            favorites: false,
            text: false,
            image: false,
            url: false,
            other: false,
        },
        ..DisplayPreference::default()
    };
    assert!(!none.valid());
    assert!(preferences.set_display(none).is_err());

    let only_all = DisplayPreference {
        show_category_filter: true,
        category_tabs: CategoryTabVisibility {
            all: true,
            favorites: false,
            text: false,
            image: false,
            url: false,
            other: false,
        },
        ..DisplayPreference::default()
    };
    assert!(!only_all.valid());
    assert!(preferences.set_display(only_all).is_err());

    let all_disabled = DisplayPreference {
        show_category_filter: true,
        category_tabs: CategoryTabVisibility {
            all: false,
            favorites: true,
            text: true,
            image: false,
            url: false,
            other: false,
        },
        ..DisplayPreference::default()
    };
    assert!(!all_disabled.valid());
    assert!(preferences.set_display(all_disabled).is_err());

    let minimal = DisplayPreference {
        show_category_filter: true,
        category_tabs: CategoryTabVisibility {
            all: true,
            favorites: true,
            text: false,
            image: false,
            url: false,
            other: false,
        },
        ..DisplayPreference::default()
    };
    assert!(minimal.valid());
    preferences.set_display(minimal)?;
    Ok(())
}

#[test]
fn todo_preferences_survive_restart_and_recover_invalid_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.todo_preference()?, TodoPreference::default());
    let todo = TodoPreference {
        density: TodoDensity::Spacious,
        show_created: false,
        show_edited: false,
        show_completed: false,
        toast: true,
    };
    preferences.set_todo_preference(todo)?;
    drop(db);
    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.todo_preference()?, todo);
    assert_eq!(
        preferences.todo_preference()?.density(CardDensity::Compact),
        CardDensity::Spacious
    );
    assert_eq!(
        TodoPreference::default().density(CardDensity::Compact),
        CardDensity::Compact
    );
    SettingsRepository::new(&db).set(TODO_KEY, "invalid")?;
    assert_eq!(preferences.todo_preference()?, TodoPreference::default());
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
