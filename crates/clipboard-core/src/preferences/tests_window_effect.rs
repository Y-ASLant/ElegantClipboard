use super::*;
use crate::{History, backup::restore_backup};

#[test]
fn window_effect_persists_and_rejects_out_of_range_opacity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let preferences = Preferences::new(&db);
    assert_eq!(
        preferences.window_effect()?,
        WindowEffectPreference::default()
    );
    let preference = WindowEffectPreference {
        effect: WindowEffect::Frosted,
        opacity: 40,
    };
    preferences.set_window_effect(preference)?;
    drop(preferences);
    drop(db);

    let db = Database::new(path)?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.window_effect()?, preference);
    assert!(
        preferences
            .set_window_effect(WindowEffectPreference {
                opacity: MAX_WINDOW_EFFECT_OPACITY + 1,
                ..preference
            })
            .is_err()
    );
    assert_eq!(preferences.window_effect()?, preference);
    Ok(())
}

#[test]
fn invalid_stored_materials_fall_back_without_rewriting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let repository = SettingsRepository::new(&db);
    let preferences = Preferences::new(&db);
    for invalid in [
        "broken",
        "null",
        r#"{"effect":"unknown","opacity":40}"#,
        r#"{"effect":"none","opacity":40}"#,
        r#"{"effect":"acrylic","opacity":40}"#,
        r#"{"effect":"mica_alt","opacity":55}"#,
        r#"{"effect":"frosted","opacity":101}"#,
        r#"{"effect":"acrylic","opacity":101}"#,
        r#"{"effect":"mica_alt","opacity":-1}"#,
        r#"{"effect":"mica_alt","opacity":256}"#,
        r#"{"effect":"none","opacity":null}"#,
        r#"{"effect":"none","opacity":"40"}"#,
        r#"{"effect":"acrylic","opacity":40.5}"#,
        r#"{"effect":"none","effect":"acrylic","opacity":40}"#,
        r#"{"effect":"acrylic","opacity":40,"opacity":50}"#,
        r#"{"effect":"mica_alt","opacity":40} trailing"#,
    ] {
        repository.set(WINDOW_EFFECT_KEY, invalid)?;
        assert_eq!(
            preferences.window_effect()?,
            WindowEffectPreference::default()
        );
        assert_eq!(repository.get(WINDOW_EFFECT_KEY)?.as_deref(), Some(invalid));
    }
    Ok(())
}

#[test]
fn canonical_material_reads_do_not_write_and_failed_updates_preserve_saved_value() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let preferences = Preferences::new(&db);
    let preference = WindowEffectPreference {
        effect: WindowEffect::Mica,
        opacity: 63,
    };
    preferences.set_window_effect(preference)?;
    db.write_connection()
        .lock()
        .execute_batch("PRAGMA query_only = ON")?;
    assert_eq!(preferences.window_effect()?, preference);
    assert!(
        preferences
            .set_window_effect(WindowEffectPreference {
                effect: WindowEffect::Opaque,
                ..preference
            })
            .is_err()
    );
    assert_eq!(preferences.window_effect()?, preference);
    Ok(())
}

#[test]
fn material_read_failure_is_not_disguised_as_a_default() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    db.write_connection()
        .lock()
        .execute_batch("DROP TABLE settings")?;
    assert!(Preferences::new(&db).window_effect().is_err());
    Ok(())
}

#[test]
fn backup_restore_preserves_current_material_without_rewriting() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("source.db"))?;
    let stored = r#"{"effect":"frosted","opacity":35}"#;
    SettingsRepository::new(&history.db).set(WINDOW_EFFECT_KEY, stored)?;
    let backup = directory.path().join("history.zip");
    history.export_backup(&backup)?;
    let target = directory.path().join("restored");
    std::fs::create_dir(&target)?;
    restore_backup(&backup, &target)?;
    let restored = History::open(target.join("clipboard.db"))?;
    assert_eq!(
        Preferences::new(&restored.db).window_effect()?,
        WindowEffectPreference {
            effect: WindowEffect::Frosted,
            opacity: 35
        }
    );
    assert_eq!(
        SettingsRepository::new(&restored.db).get(WINDOW_EFFECT_KEY)?,
        Some(stored.to_owned())
    );
    assert_eq!(
        SettingsRepository::new(&history.db).get(WINDOW_EFFECT_KEY)?,
        Some(stored.to_owned())
    );
    Ok(())
}
