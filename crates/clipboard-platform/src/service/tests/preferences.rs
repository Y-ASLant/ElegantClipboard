use super::*;
use clipboard_core::{
    MAX_TEXT_BYTES,
    preferences::{HotkeyPreference, ThemePreference},
};

#[test]
fn capture_error_does_not_replace_an_unrelated_settings_acknowledgement() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);

    service.send(Command::Capture("x".repeat(MAX_TEXT_BYTES + 1)))?;
    service.send(Command::SetTheme(ThemePreference::Dark))?;
    assert!(matches!(next_event(&events), Event::BackgroundError(_)));
    assert!(matches!(
        next_event(&events),
        Event::ThemeSaved(Ok(ThemePreference::Dark))
    ));
    Ok(())
}

#[test]
fn hotkey_change_is_acknowledged_and_loaded_on_restart() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        service.initial_preferences.hotkey,
        HotkeyPreference::default()
    );
    next_snapshot(&events, 0);
    service.send(Command::SetHotkey(HotkeyPreference::Shortcut(
        " alt + control + f9 ".into(),
    )))?;
    wait_for(
        &events,
        Duration::from_secs(5),
        "hotkey save timed out",
        |event| match event {
            Event::HotkeySaved(result) => {
                assert_eq!(
                    result.unwrap(),
                    HotkeyPreference::Shortcut("Ctrl+Alt+F9".into())
                );
                Some(())
            }
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    for rejected in ["Alt+Numpad1", "Ctrl+Ctrl+V", ""] {
        service.send(Command::SetHotkey(HotkeyPreference::Shortcut(
            rejected.into(),
        )))?;
        wait_for(
            &events,
            Duration::from_secs(5),
            "hotkey rejection timed out",
            |event| match event {
                Event::HotkeySaved(result) => {
                    assert!(result.is_err(), "{rejected}");
                    Some(())
                }
                Event::Error(message) => panic!("{message}"),
                _ => None,
            },
        );
    }
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(
        reopened.initial_preferences.hotkey,
        HotkeyPreference::Shortcut("Ctrl+Alt+F9".into())
    );
    Ok(())
}

#[test]
fn paste_shortcuts_save_atomically_and_reject_window_hotkey_conflicts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    let mut shortcuts = service.initial_preferences.paste_shortcuts.clone();
    shortcuts.set_slot(true, 10, "Ctrl+Shift+Z".into())?;
    service.send(Command::SetPasteShortcuts(Box::new(shortcuts.clone())))?;
    let result = wait_for(
        &events,
        Duration::from_secs(5),
        "paste shortcuts save timed out",
        |event| match event {
            Event::PasteShortcutsSaved(result) => Some(result),
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    assert_eq!(*result.unwrap(), shortcuts);
    let mut invalid = shortcuts.clone();
    invalid.set_slot(true, 10, service.initial_preferences.hotkey.label().into())?;
    service.send(Command::SetPasteShortcuts(Box::new(invalid)))?;
    let result = wait_for(
        &events,
        Duration::from_secs(5),
        "paste shortcuts rejection timed out",
        |event| match event {
            Event::PasteShortcutsSaved(result) => Some(result),
            Event::Error(message) => panic!("{message}"),
            _ => None,
        },
    );
    assert!(result.is_err());
    drop(service);
    let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(reopened.initial_preferences.paste_shortcuts, shortcuts);
    Ok(())
}

#[test]
fn pause_acknowledgement_is_ordered_and_does_not_change_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert!(!service.initial_preferences.paused);
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
    assert!(reopened.initial_preferences.paused);
    Ok(())
}
