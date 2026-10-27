use super::*;
use crate::{History, NativeFormatCapture};
use rusqlite::types::Value as SqlValue;
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

mod failures;

type Rows = Vec<Vec<SqlValue>>;

struct Fixture {
    _directory: TempDir,
    target: PathBuf,
    roots: Vec<PathBuf>,
    outside: PathBuf,
    payload: String,
}

impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let old = directory.path().join("ElegantClipboardGPUI");
        let target = directory.path().join("ElegantClipboard");
        let outside = directory.path().join("user-documents/missing.txt");
        for name in ["images", "icons", "staged"] {
            fs::create_dir_all(old.join(name))?;
        }
        let canonical = fs::canonicalize(&old)?;
        fs::write(old.join("images/preview.png"), b"image bytes")?;
        fs::write(old.join("icons/editor.ico"), b"icon bytes")?;
        fs::write(old.join("staged/copy.txt"), b"attachment bytes")?;
        let mut payload = json!({
            "staged": [
                {
                    "original": old.join("staged/original.txt"),
                    "staged": canonical.join("staged/copy.txt"),
                    "size": 16,
                    "metadata": {"original": old.join("staged/original.txt")}
                },
                {"original": outside, "staged": old.join("staged/external-copy.txt")}
            ],
            "opaque": {"original": old.join("staged/original.txt")},
            "native": [1, 2, 255]
        })
        .to_string();
        payload.pop();
        payload.push_str(", \"large\":1234567890123456789012345678901234567890, \"precise\":0.123456789012345678901234567890 }");
        let database = Database::new(old.join("clipboard.db"))?;
        {
            let writer = database.write_connection();
            let connection = writer.lock();
            connection.execute_batch(
                "INSERT INTO groups (id, name, color, sort_order, created_at)
                 VALUES (7, 'work', '#123456', 8, '2002-01-01 01:02:03');
                 INSERT INTO settings (key, value, updated_at) VALUES
                 ('secret_token', 'keep this secret', '2001-02-03 04:05:06'),
                 ('gpui_theme_mode', 'dark', '2001-02-03 04:05:06'),
                 ('legacy_future_setting', '{\"opaque\":true}', '2001-02-03 04:05:06');",
            )?;
            connection.execute(
                "INSERT INTO clipboard_items (
                    id, content_type, text_content, html_content, rtf_content,
                    image_path, source_app_icon, file_paths, file_payload,
                    content_hash, semantic_hash, preview, byte_size, image_width,
                    image_height, is_pinned, is_favorite, favorite_order, sort_order,
                    created_at, updated_at, access_count, last_accessed_at, char_count,
                    source_app_name, note, group_id)
                 VALUES (41, 'files', ?1, '<b>keep</b>', '{rtf}', ?2, ?3, ?4, ?5,
                    'unchanged-hash', 'unchanged-semantic', 'preview', 4321, 640, 480,
                    1, 1, 87, 123, '2003-04-05 06:07:08', '2004-05-06 07:08:09',
                    19, '2005-06-07 08:09:10', 55, 'editor', '备注不能丢失', 7)",
                params![
                    format!("正文含旧路径 {}", old.display()),
                    old.join("images/preview.png").to_str(),
                    canonical.join("icons/editor.ico").to_str(),
                    json!([
                        old.join("staged/original.txt"),
                        outside,
                        directory.path().join("ElegantClipboardGPUI-other/user.txt"),
                        "relative/user.txt"
                    ])
                    .to_string(),
                    payload
                ],
            )?;
            connection.execute(
                "INSERT INTO clipboard_items (id, content_type, image_path, source_app_icon,
                    file_paths, file_payload, content_hash, semantic_hash,
                    created_at, updated_at)
                 VALUES (42, 'image', ?1, ?1, ?2, ?3, 'outside', 'outside',
                    '2006-07-08 09:10:11', '2007-08-09 10:11:12')",
                params![
                    outside.to_str(),
                    json!([outside]).to_string(),
                    format!(r#"{{ "staged": [], "text": {} }}"#, json!(old))
                ],
            )?;
            connection.execute_batch(
                "INSERT INTO content_hash_aliases (alias_hash, canonical_hash, created_at)
                 VALUES ('alias', 'unchanged-hash', '2008-09-10 11:12:13');",
            )?;
        }
        let history = History::new(&database);
        history.attach_native_formats(
            41,
            vec![
                NativeFormatCapture {
                    name: "Inline".into(),
                    data: vec![0, 1, 2, 255],
                },
                NativeFormatCapture {
                    name: "Blob".into(),
                    data: vec![17; 96 * 1024],
                },
            ],
        )?;
        drop(history);
        crate::import::checkpoint_staged_database(&database.write_connection().lock())?;
        drop(database);
        fs::rename(&old, &target)?;
        Ok(Self {
            _directory: directory,
            target,
            roots: vec![old, canonical],
            outside,
            payload,
        })
    }

    fn connection(&self) -> Result<Connection> {
        Ok(Connection::open(self.target.join("clipboard.db"))?)
    }

    fn relocate(&self) -> Result<()> {
        relocate_managed_paths(&self.target, &self.roots)
    }

    fn assert_unlocked(&self) -> Result<()> {
        let rollback = self.target.with_extension("rollback");
        fs::rename(&self.target, &rollback)?;
        fs::rename(rollback, &self.target)?;
        Ok(())
    }
}

fn rows(connection: &Connection, sql: &str) -> Result<Rows> {
    let mut query = connection.prepare(sql)?;
    let columns = query.column_count();
    Ok(query
        .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())?
        .collect::<Result<_, _>>()?)
}

fn snapshot(connection: &Connection) -> Result<Vec<Rows>> {
    [
        "SELECT * FROM clipboard_items ORDER BY id",
        "SELECT * FROM groups ORDER BY id",
        "SELECT * FROM settings ORDER BY key",
        "SELECT * FROM native_formats ORDER BY item_id, format_name",
        "SELECT * FROM content_hash_aliases ORDER BY alias_hash",
        "SELECT name, sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name",
    ]
    .into_iter()
    .map(|sql| rows(connection, sql))
    .collect()
}

fn metadata(connection: &Connection) -> Result<Vec<Rows>> {
    let mut result = snapshot(connection)?;
    result[0] = rows(
        connection,
        "SELECT id, content_type, text_content, html_content, rtf_content,
         content_hash, semantic_hash, preview, byte_size, image_width, image_height,
         is_pinned, is_favorite, favorite_order, sort_order, created_at, updated_at,
         access_count, last_accessed_at, char_count, source_app_name, note, group_id
         FROM clipboard_items ORDER BY id",
    )?;
    Ok(result)
}

#[test]
fn moved_tree_rebases_only_managed_references_and_preserves_history_and_blobs() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = metadata(&fixture.connection()?)?;
    let external_row = rows(
        &fixture.connection()?,
        "SELECT * FROM clipboard_items WHERE id = 42",
    )?;
    let native_bytes =
        History::open(fixture.target.join("clipboard.db"))?.native_formats_for_copy(41)?;
    fixture.relocate()?;
    fixture.assert_unlocked()?;
    let canonical = fs::canonicalize(&fixture.target)?;
    let database = Database::new(fixture.target.join("clipboard.db"))?;
    let history = History::new(&database);
    let item = history.item(41)?;
    assert_eq!(
        item.image_path,
        canonical
            .join("images/preview.png")
            .to_str()
            .map(str::to_owned)
    );
    assert_eq!(
        item.source_app_icon,
        canonical
            .join("icons/editor.ico")
            .to_str()
            .map(str::to_owned)
    );
    let paths: Vec<String> = serde_json::from_str(item.file_paths.as_deref().unwrap())?;
    assert_eq!(
        paths[0],
        canonical.join("staged/original.txt").to_str().unwrap()
    );
    assert_eq!(paths[1], fixture.outside.to_str().unwrap());
    assert!(paths[2].contains("ElegantClipboardGPUI-other"));
    assert_eq!(paths[3], "relative/user.txt");
    let raw = item.file_payload.unwrap();
    let payload: Value = serde_json::from_str(&raw)?;
    assert_eq!(
        payload["staged"][0]["staged"],
        json!(canonical.join("staged/copy.txt"))
    );
    assert_eq!(
        payload["staged"][0]["original"],
        json!(canonical.join("staged/original.txt"))
    );
    assert_eq!(payload["staged"][1]["original"], json!(fixture.outside));
    assert_eq!(
        payload["staged"][1]["staged"],
        json!(canonical.join("staged/external-copy.txt"))
    );
    let original: Value = serde_json::from_str(&fixture.payload)?;
    assert_eq!(payload["opaque"], original["opaque"]);
    assert_eq!(
        payload["staged"][0]["metadata"],
        original["staged"][0]["metadata"]
    );
    assert!(raw.ends_with(", \"large\":1234567890123456789012345678901234567890, \"precise\":0.123456789012345678901234567890 }"));
    assert_eq!(history.native_formats_for_copy(41)?, native_bytes);
    assert_eq!(
        fs::read(canonical.join("staged/copy.txt"))?,
        b"attachment bytes"
    );
    assert!(!fixture.outside.exists());
    drop(history);
    drop(database);
    assert_eq!(metadata(&fixture.connection()?)?, before);
    assert_eq!(
        rows(
            &fixture.connection()?,
            "SELECT * FROM clipboard_items WHERE id = 42"
        )?,
        external_row
    );
    Ok(())
}

#[test]
fn repeating_relocation_preserves_persisted_rows_and_timestamp_trigger() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.relocate()?;
    let first = snapshot(&fixture.connection()?)?;
    fixture.relocate()?;
    fixture.relocate()?;
    fixture.assert_unlocked()?;
    assert_eq!(snapshot(&fixture.connection()?)?, first);
    let connection = fixture.connection()?;
    connection.execute(
        "UPDATE clipboard_items SET note = 'after migration' WHERE id = 41",
        [],
    )?;
    let updated: String = connection.query_row(
        "SELECT updated_at FROM clipboard_items WHERE id = 41",
        [],
        |row| row.get(0),
    )?;
    assert_ne!(updated, "2004-05-06 07:08:09");
    Ok(())
}

#[test]
fn upgrades_missing_optional_columns_before_relocating() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.connection()?.execute_batch(
        "ALTER TABLE clipboard_items DROP COLUMN file_payload;
         ALTER TABLE clipboard_items DROP COLUMN note;",
    )?;
    fixture.relocate()?;
    let history = History::open(fixture.target.join("clipboard.db"))?;
    let item = history.item(41)?;
    assert_eq!(
        item.image_path,
        fs::canonicalize(&fixture.target)?
            .join("images/preview.png")
            .to_str()
            .map(str::to_owned)
    );
    assert_eq!(item.note, None);
    assert_eq!(item.file_payload, None);
    assert_eq!(item.updated_at, "2004-05-06 07:08:09");
    Ok(())
}
