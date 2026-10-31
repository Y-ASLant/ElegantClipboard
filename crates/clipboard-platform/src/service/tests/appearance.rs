use super::*;
use clipboard_core::preferences::{CornerRadiusPreference, ThemePreference};

#[test]
fn theme_acknowledgement_is_persisted_before_reopening() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    assert_eq!(service.initial_preferences.theme, ThemePreference::System);
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
    assert_eq!(reopened.initial_preferences.theme, ThemePreference::Dark);
    assert!(next_snapshot(&events, 0).is_empty());
    Ok(())
}

#[test]
fn corner_radius_acknowledgement_preserves_value_after_rejected_change() -> Result<()> {
    let directory = tempfile::tempdir()?;
    for preference in [
        CornerRadiusPreference::Fixed(12),
        CornerRadiusPreference::System,
    ] {
        let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
        next_snapshot(&events, 0);
        service.send(Command::SetCornerRadius(preference))?;
        let saved = wait_for(
            &events,
            Duration::from_secs(5),
            "radius save timed out",
            |event| match event {
                Event::CornerRadiusSaved(result) => Some(result),
                _ => None,
            },
        );
        assert_eq!(saved.unwrap(), preference);
        for radius in [8, 17] {
            service.send(Command::SetCornerRadius(CornerRadiusPreference::Fixed(
                radius,
            )))?;
            let rejected = wait_for(
                &events,
                Duration::from_secs(5),
                "radius rejection timed out",
                |event| match event {
                    Event::CornerRadiusSaved(result) => Some(result),
                    _ => None,
                },
            );
            assert!(rejected.is_err());
        }
        drop(service);
        let (reopened, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(reopened.initial_preferences.corner_radius, preference);
    }
    Ok(())
}

#[test]
fn startup_migrates_legacy_eight_and_preserves_other_saved_radii() -> Result<()> {
    use clipboard_core::database::{Database, SettingsRepository};

    for (stored, expected, canonical) in [
        ("8", CornerRadiusPreference::System, "system"),
        ("12", CornerRadiusPreference::Fixed(12), "12"),
    ] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let db = Database::new(path.clone())?;
        SettingsRepository::new(&db).set("gpui_corner_radius", stored)?;
        drop(db);

        let (service, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_preferences.corner_radius, expected);
        drop(service);

        let db = Database::new(path)?;
        assert_eq!(
            SettingsRepository::new(&db)
                .get("gpui_corner_radius")?
                .as_deref(),
            Some(canonical)
        );
        drop(db);
        let (service, _) = Service::start(Some(directory.path().to_owned()), false)?;
        assert_eq!(service.initial_preferences.corner_radius, expected);
    }
    Ok(())
}
