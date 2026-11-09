use super::*;
use crate::database::Database;
use anyhow::Result;

#[test]
fn fonts_default_persist_and_reject_invalid_sizes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("prefs.db"))?;
    let preferences = Preferences::new(&db);
    assert_eq!(preferences.fonts()?, FontPreference::default());

    let custom = FontPreference {
        ui_font: "Segoe UI".into(),
        ui_font_size: 16,
        card_font: "Consolas".into(),
        card_font_size: 15,
        preview_font: String::new(),
        preview_font_size: 13,
    };
    preferences.set_fonts(custom.clone())?;
    assert_eq!(preferences.fonts()?, custom);

    let invalid = FontPreference {
        ui_font_size: 11,
        ..FontPreference::default()
    };
    assert!(preferences.set_fonts(invalid).is_err());
    assert_eq!(preferences.fonts()?, custom);

    SettingsRepository::new(&db).set(FONT_KEY, "not-json")?;
    assert_eq!(preferences.fonts()?, FontPreference::default());

    Ok(())
}
