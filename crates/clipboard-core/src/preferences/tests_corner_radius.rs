use super::*;
use crate::{History, backup::restore_backup};

#[test]
fn corner_radius_persists_across_restart_and_rejects_removed_or_invalid_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    for preference in [
        CornerRadiusPreference::Fixed(0),
        CornerRadiusPreference::Fixed(4),
        CornerRadiusPreference::Fixed(12),
        CornerRadiusPreference::Fixed(16),
        CornerRadiusPreference::System,
    ] {
        let db = Database::new(path.clone())?;
        Preferences::new(&db).set_corner_radius(preference)?;
        drop(db);

        let db = Database::new(path.clone())?;
        let preferences = Preferences::new(&db);
        assert_eq!(preferences.corner_radius()?, preference);
        let stored = SettingsRepository::new(&db).get(CORNER_RADIUS_KEY)?;
        for radius in [8, 17, u8::MAX] {
            assert!(
                preferences
                    .set_corner_radius(CornerRadiusPreference::Fixed(radius))
                    .is_err()
            );
            assert_eq!(preferences.corner_radius()?, preference);
            assert_eq!(SettingsRepository::new(&db).get(CORNER_RADIUS_KEY)?, stored);
        }
    }
    Ok(())
}

#[test]
fn legacy_radii_preserve_supported_values_and_normalize_eight_on_disk() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    for radius in 0..=16 {
        let db = Database::new(path.clone())?;
        let stored = radius.to_string();
        SettingsRepository::new(&db).set(CORNER_RADIUS_KEY, &stored)?;
        let expected = if radius == 8 {
            CornerRadiusPreference::System
        } else {
            CornerRadiusPreference::Fixed(radius)
        };
        let canonical = if radius == 8 { "system" } else { &stored };
        assert_eq!(Preferences::new(&db).corner_radius()?, expected);
        assert_eq!(
            SettingsRepository::new(&db)
                .get(CORNER_RADIUS_KEY)?
                .as_deref(),
            Some(canonical)
        );
        drop(db);

        let db = Database::new(path.clone())?;
        assert_eq!(Preferences::new(&db).corner_radius()?, expected);
        assert_eq!(
            SettingsRepository::new(&db)
                .get(CORNER_RADIUS_KEY)?
                .as_deref(),
            Some(canonical)
        );
    }
    Ok(())
}

#[test]
fn invalid_stored_radii_fall_back_without_rewriting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let repository = SettingsRepository::new(&db);
    let preferences = Preferences::new(&db);
    for invalid in ["-1", "17", "256", "broken", "", "8.0"] {
        repository.set(CORNER_RADIUS_KEY, invalid)?;
        assert_eq!(
            preferences.corner_radius()?,
            CornerRadiusPreference::default()
        );
        assert_eq!(repository.get(CORNER_RADIUS_KEY)?.as_deref(), Some(invalid));
    }
    Ok(())
}

#[test]
fn canonical_radius_reads_do_not_write_and_failed_updates_preserve_saved_value() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let preferences = Preferences::new(&db);
    for preference in [
        CornerRadiusPreference::Fixed(12),
        CornerRadiusPreference::System,
    ] {
        preferences.set_corner_radius(preference)?;
        db.write_connection()
            .lock()
            .execute_batch("PRAGMA query_only = ON")?;
        assert_eq!(preferences.corner_radius()?, preference);
        assert!(
            preferences
                .set_corner_radius(CornerRadiusPreference::Fixed(0))
                .is_err()
        );
        assert_eq!(preferences.corner_radius()?, preference);
        db.write_connection()
            .lock()
            .execute_batch("PRAGMA query_only = OFF")?;
    }
    Ok(())
}

#[test]
fn failed_radius_migration_retains_legacy_value_and_can_be_retried() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let repository = SettingsRepository::new(&db);
    repository.set(CORNER_RADIUS_KEY, "8")?;
    db.write_connection()
        .lock()
        .execute_batch("PRAGMA query_only = ON")?;
    assert!(Preferences::new(&db).corner_radius().is_err());
    assert_eq!(repository.get(CORNER_RADIUS_KEY)?.as_deref(), Some("8"));
    db.write_connection()
        .lock()
        .execute_batch("PRAGMA query_only = OFF")?;
    assert_eq!(
        Preferences::new(&db).corner_radius()?,
        CornerRadiusPreference::System
    );
    assert_eq!(
        repository.get(CORNER_RADIUS_KEY)?.as_deref(),
        Some("system")
    );
    Ok(())
}

#[test]
fn radius_read_failure_is_not_disguised_as_a_default() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    db.write_connection()
        .lock()
        .execute_batch("DROP TABLE settings")?;
    assert!(Preferences::new(&db).corner_radius().is_err());
    Ok(())
}

#[test]
fn backup_restore_preserves_system_radius_and_normalizes_legacy_eight() -> Result<()> {
    for stored in ["system", "8"] {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("source.db"))?;
        SettingsRepository::new(&history.db).set(CORNER_RADIUS_KEY, stored)?;
        let backup = directory.path().join("history.zip");
        history.export_backup(&backup)?;
        let target = directory.path().join("restored");
        std::fs::create_dir(&target)?;
        restore_backup(&backup, &target)?;
        let restored = History::open(target.join("clipboard.db"))?;
        assert_eq!(
            Preferences::new(&restored.db).corner_radius()?,
            CornerRadiusPreference::System
        );
        assert_eq!(
            SettingsRepository::new(&restored.db)
                .get(CORNER_RADIUS_KEY)?
                .as_deref(),
            Some("system")
        );
        assert_eq!(
            SettingsRepository::new(&history.db)
                .get(CORNER_RADIUS_KEY)?
                .as_deref(),
            Some(stored)
        );
    }
    Ok(())
}
