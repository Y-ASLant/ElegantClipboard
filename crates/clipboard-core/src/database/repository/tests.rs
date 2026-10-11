use super::*;
use crate::database::{GroupRepository, SettingsRepository};
use rusqlite::params;

mod groups;
mod items;
mod query;
mod retention;
mod settings;

fn temp_db() -> Database {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("ec_repo_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = dir.join(format!(
        "test_{}_{}.db",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        n
    ));
    Database::new(path).unwrap()
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
