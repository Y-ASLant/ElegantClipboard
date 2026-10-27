use crate::{History, legacy_backup::import_legacy_backup};
use anyhow::Result;
use rusqlite::params;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn old_zip_preserves_unbundled_paths_and_rebases_windows_basename() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("source.db");
    let history = History::open(source.clone())?;
    let bundled = history.capture("bundled media")?.unwrap();
    let missing = history.capture("unbundled media")?.unwrap();
    let missing_image = r"C:\old\images\missing.png";
    let missing_icon = r"C:\old\icons\missing.ico";
    let payload = serde_json::json!({
        "staged": [
            {"staged": r"C:\old\STAGED\draft.txt", "size": 7},
            {"staged": r"C:\old\staged\missing.txt", "size": 9}
        ],
        "metadata": "preserved"
    })
    .to_string();
    {
        let connection = history.db.write_connection();
        let connection = connection.lock();
        connection.execute(
            "UPDATE clipboard_items SET image_path = ?1, source_app_icon = ?2,
             file_payload = ?3 WHERE id = ?4",
            params![r"C:\old\IMAGES\photo.png", missing_icon, payload, bundled],
        )?;
        connection.execute(
            "UPDATE clipboard_items SET image_path = ?1 WHERE id = ?2",
            params![missing_image, missing],
        )?;
    }
    crate::import::checkpoint_staged_database(&history.db.write_connection().lock())?;
    drop(history);
    let archive_path = directory.path().join("legacy.zip");
    let mut zip = ZipWriter::new(File::create(&archive_path)?);
    zip.start_file("clipboard.db", SimpleFileOptions::default())?;
    zip.write_all(&fs::read(source)?)?;
    let png = b"\x89PNG\r\n\x1a\nsynthetic";
    zip.start_file("images/photo.png", SimpleFileOptions::default())?;
    zip.write_all(png)?;
    zip.start_file("staged/draft.txt", SimpleFileOptions::default())?;
    zip.write_all(b"draft")?;
    zip.finish()?;

    let target = directory.path().join("restored");
    fs::create_dir_all(&target)?;
    let report = import_legacy_backup(&archive_path, &target)?;
    assert_eq!(report.unbundled_images, 1);
    let restored = History::open(target.join("clipboard.db"))?;
    let item = restored.item(bundled)?;
    let image = PathBuf::from(item.image_path.unwrap());
    assert!(image.starts_with(fs::canonicalize(target.join("images"))?));
    assert_eq!(fs::read(image)?, png);
    assert_eq!(item.source_app_icon.as_deref(), Some(missing_icon));
    assert_eq!(
        restored.item(missing)?.image_path.as_deref(),
        Some(missing_image)
    );
    let mapped: Value = serde_json::from_str(&item.file_payload.unwrap())?;
    assert!(mapped.get("staged").is_none());
    assert!(!target.join("staged").exists());
    assert_eq!(mapped["metadata"], "preserved");
    Ok(())
}
