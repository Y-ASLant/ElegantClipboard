use super::*;
use crate::database::{GroupRepository, SettingsRepository};
use rusqlite::params;

mod groups;
mod items;
mod query;
mod retention;
mod settings;

fn temp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("test.db")).unwrap();
    (dir, db)
}

fn make_text_item(text: &str) -> NewClipboardItem {
    let hash = blake3::hash(format!("text:{text}").as_bytes())
        .to_hex()
        .to_string();
    NewClipboardItem {
        content_type: ContentType::Text,
        text_content: Some(text.to_string()),
        preview: Some(text.chars().take(200).collect()),
        content_hash: hash.clone(),
        semantic_hash: hash,
        byte_size: text.len() as i64,
        char_count: Some(text.chars().count() as i64),
        ..Default::default()
    }
}
