use super::*;
use clipboard_core::{
    MAX_TEXT_BYTES,
    preferences::{
        AudioPreference, DisplayPreference, HotkeyPreference, HoverPreviewPreference,
        LanguagePreference, PasteKeyPreference, ThemePreference, WindowPositionPreference,
        WindowSizePreference,
    },
};

#[test]
fn capture_error_does_not_replace_an_unrelated_settings_acknowledgement() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);

    service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(events.recv_blocking()?, Event::BackgroundError(_)));
    assert!(matches!(
        events.recv_blocking()?,
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn theme_acknowledgement_is_persisted_before_reopening() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_theme, ThemePreference::System);
    next_snapshot(&events, 0);
    for theme in [
        ThemePreference::Light,
        ThemePreference::System,
        ThemePreference::Dark,
    ] {
        service.send(Command::SetTheme(theme))?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "theme timed out",
            |event| match event {
                Event::ThemeSaved(result) => {
                    assert_eq!(result.unwrap(), theme);
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    drop(service);
    let (reopened, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_theme, ThemePreference::Dark);
    assert!(next_snapshot(&events, 0).is_empty());
    Ok(())
}

#[test]
fn language_defaults_to_chinese_and_is_loaded_after_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_language, LanguagePreference::Chinese);
    next_snapshot(&events, 0);
    service.send(Command::SetLanguage(LanguagePreference::English))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "language save timed out",
        |event| match event {
            Event::LanguageSaved(result) => {
                assert_eq!(result.unwrap(), LanguagePreference::English);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_language, LanguagePreference::English);
    Ok(())
}

#[test]
fn hotkey_change_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_hotkey, HotkeyPreference::CtrlShiftV);
    next_snapshot(&events, 0);
    service.send(Command::SetHotkey(HotkeyPreference::AltC))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hotkey save timed out",
        |event| match event {
            Event::HotkeySaved(result) => {
                assert_eq!(result.unwrap(), HotkeyPreference::AltC);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_hotkey, HotkeyPreference::AltC);
    Ok(())
}

#[test]
fn window_size_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_window_size, None);
    next_snapshot(&events, 0);
    let size = WindowSizePreference::new(960, 720).unwrap();
    service.send(Command::SetWindowSize(size))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "window size save timed out",
        |event| match event {
            Event::WindowSizeSaved(result) => {
                assert_eq!(result.unwrap(), size);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_window_size, Some(size));
    Ok(())
}

#[test]
fn window_position_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        service.initial_window_position,
        WindowPositionPreference::FollowCursor
    );
    next_snapshot(&events, 0);
    service.send(Command::SetWindowPosition(
        WindowPositionPreference::ScreenCenter,
    ))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "window position save timed out",
        |event| match event {
            Event::WindowPositionSaved(result) => {
                assert_eq!(result.unwrap(), WindowPositionPreference::ScreenCenter);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        reopened.initial_window_position,
        WindowPositionPreference::ScreenCenter
    );
    Ok(())
}

#[test]
fn window_behavior_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(service.initial_persist_window_size);
    assert!(!service.initial_auto_reset_state);
    assert!(!service.initial_search_auto_focus);
    assert!(service.initial_search_auto_clear);
    assert!(!service.initial_skip_clear_confirm);
    assert!(service.initial_paste_close_window);
    assert_eq!(service.initial_paste_key, PasteKeyPreference::CtrlV);
    assert!(service.initial_paste_move_to_top);
    assert!(!service.initial_quick_paste_enabled);
    next_snapshot(&events, 0);
    service.send(Command::SetPersistWindowSize(false))?;
    service.send(Command::SetAutoResetState(true))?;
    service.send(Command::SetSearchAutoFocus(true))?;
    service.send(Command::SetSearchAutoClear(false))?;
    service.send(Command::SetSkipClearConfirm(true))?;
    service.send(Command::SetPasteCloseWindow(false))?;
    service.send(Command::SetPasteKey(PasteKeyPreference::ShiftInsert))?;
    service.send(Command::SetPasteMoveToTop(false))?;
    service.send(Command::SetQuickPasteEnabled(true))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut size_acknowledged = false;
    let mut reset_acknowledged = false;
    let mut focus_acknowledged = false;
    let mut clear_acknowledged = false;
    let mut skip_clear_acknowledged = false;
    let mut paste_acknowledged = false;
    let mut paste_key_acknowledged = false;
    let mut move_acknowledged = false;
    let mut quick_acknowledged = false;
    while !size_acknowledged
        || !reset_acknowledged
        || !focus_acknowledged
        || !clear_acknowledged
        || !skip_clear_acknowledged
        || !paste_acknowledged
        || !paste_key_acknowledged
        || !move_acknowledged
        || !quick_acknowledged
    {
        match events.try_recv() {
            Ok(Event::PersistWindowSizeSaved(result)) => {
                assert!(!result.unwrap());
                size_acknowledged = true;
            }
            Ok(Event::AutoResetStateSaved(result)) => {
                assert!(result.unwrap());
                reset_acknowledged = true;
            }
            Ok(Event::SearchAutoFocusSaved(result)) => {
                assert!(result.unwrap());
                focus_acknowledged = true;
            }
            Ok(Event::SearchAutoClearSaved(result)) => {
                assert!(!result.unwrap());
                clear_acknowledged = true;
            }
            Ok(Event::SkipClearConfirmSaved(result)) => {
                assert!(result.unwrap());
                skip_clear_acknowledged = true;
            }
            Ok(Event::PasteCloseWindowSaved(result)) => {
                assert!(!result.unwrap());
                paste_acknowledged = true;
            }
            Ok(Event::PasteKeySaved(result)) => {
                assert_eq!(result.unwrap(), PasteKeyPreference::ShiftInsert);
                paste_key_acknowledged = true;
            }
            Ok(Event::PasteMoveToTopSaved(result)) => {
                assert!(!result.unwrap());
                move_acknowledged = true;
            }
            Ok(Event::QuickPasteEnabledSaved(result)) => {
                assert!(result.unwrap());
                quick_acknowledged = true;
            }
            Ok(Event::Error(message)) => panic!("{message}"),
            _ => {}
        }
        assert!(
            std::time::Instant::now() < deadline,
            "window behavior save timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(!reopened.initial_persist_window_size);
    assert!(reopened.initial_auto_reset_state);
    assert!(reopened.initial_search_auto_focus);
    assert!(!reopened.initial_search_auto_clear);
    assert!(reopened.initial_skip_clear_confirm);
    assert!(!reopened.initial_paste_close_window);
    assert_eq!(reopened.initial_paste_key, PasteKeyPreference::ShiftInsert);
    assert!(!reopened.initial_paste_move_to_top);
    assert!(reopened.initial_quick_paste_enabled);
    Ok(())
}

#[test]
fn paste_shortcuts_save_atomically_and_reject_window_hotkey_conflicts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let mut shortcuts = service.initial_paste_shortcuts.clone();
    shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
    service.send(Command::SetPasteShortcuts(Box::new(shortcuts.clone())))?;
    let result = loop {
        match events.recv_blocking()? {
            Event::PasteShortcutsSaved(result) => break result,
            Event::Error(message) => anyhow::bail!("{message}"),
            _ => {}
        }
    };
    assert_eq!(*result.unwrap(), shortcuts);
    let mut invalid = shortcuts.clone();
    invalid.set_slot(true, 10, "Ctrl+Shift+V".into())?;
    service.send(Command::SetPasteShortcuts(Box::new(invalid)))?;
    let result = loop {
        match events.recv_blocking()? {
            Event::PasteShortcutsSaved(result) => break result,
            Event::Error(message) => anyhow::bail!("{message}"),
            _ => {}
        }
    };
    assert!(result.is_err());
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_paste_shortcuts, shortcuts);
    Ok(())
}

#[test]
fn display_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::CardDensity;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_display, DisplayPreference::default());
    next_snapshot(&events, 0);
    let display = DisplayPreference {
        show_category_filter: false,
        show_drag_area_indicator: false,
        card_density: CardDensity::Spacious,
        card_max_lines: 5,
        show_time: false,
        time_format: clipboard_core::preferences::TimeFormat::Relative,
        show_char_count: false,
        show_byte_size: false,
        show_source_app: false,
        source_app_display: clipboard_core::preferences::SourceAppDisplay::Icon,
    };
    service.send(Command::SetDisplay(display))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "display save timed out",
        |event| match event {
            Event::DisplaySaved(result) => {
                assert_eq!(result.unwrap(), display);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_display, display);
    Ok(())
}

#[test]
fn audio_settings_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::SoundTiming;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_audio, AudioPreference::default());
    next_snapshot(&events, 0);
    let audio = AudioPreference {
        copy_enabled: true,
        copy_timing: SoundTiming::AfterSuccess,
        paste_enabled: true,
        paste_timing: SoundTiming::AfterSuccess,
    };
    service.send(Command::SetAudio(audio))?;
    assert!(matches!(events.recv_blocking()?, Event::AudioSaved(Ok(saved)) if saved == audio));
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_audio, audio);
    Ok(())
}

#[test]
fn hover_preview_preferences_are_acknowledged_and_loaded_on_restart() -> Result<()> {
    use clipboard_core::preferences::HoverPreviewPosition;

    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        service.initial_hover_preview,
        HoverPreviewPreference::default()
    );
    next_snapshot(&events, 0);
    let preference = HoverPreviewPreference {
        text: true,
        expanded_image: true,
        delay_ms: 750,
        position: HoverPreviewPosition::Left,
        zoom_step: 20,
        ..HoverPreviewPreference::default()
    };
    service.send(Command::SetHoverPreview(preference))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hover settings timed out",
        |event| match event {
            Event::HoverPreviewSaved(result) => {
                assert_eq!(result.unwrap(), preference);
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_hover_preview, preference);
    Ok(())
}

#[test]
fn pause_acknowledgement_is_ordered_and_does_not_change_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(!service.initial_paused);
    next_snapshot(&events, 0);
    for paused in [true, false, true] {
        service.send(Command::Pause(paused))?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "pause was not acknowledged",
            |event| match event {
                Event::Paused(actual) => {
                    assert_eq!(actual, paused);
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    assert!(next_snapshot(&events, 1).is_empty());
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(reopened.initial_paused);
    Ok(())
}
