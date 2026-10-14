mod migration_recovery;

use super::*;

fn temp_db() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().join("test.db")).unwrap();
    (dir, db)
}

#[test]
fn read_write_separation() {
    let (_dir, db) = temp_db();
    {
        let w = db.write_connection();
        let w = w.lock();
        w.execute(
            "INSERT INTO settings (key, value) VALUES ('test_rw', 'hello')",
            [],
        )
        .unwrap();
    }
    let r = db.read_connection();
    let r = r.lock();
    let val: String = r
        .query_row(
            "SELECT value FROM settings WHERE key = 'test_rw'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(val, "hello");
}

#[test]
fn clone_shares_connections() {
    let (_dir, db) = temp_db();
    let db2 = db.clone();
    {
        let w = db.write_connection();
        let w = w.lock();
        w.execute(
            "INSERT INTO settings (key, value) VALUES ('clone_test', 'shared')",
            [],
        )
        .unwrap();
    }
    let r = db2.read_connection();
    let r = r.lock();
    let val: String = r
        .query_row(
            "SELECT value FROM settings WHERE key = 'clone_test'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(val, "shared");
}

#[test]
fn orphaned_clipboard_items_table_is_recovered_before_schema_creation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("orphan.db");
    let db = Database::new(path.clone()).unwrap();
    db.write_connection()
        .lock()
        .execute(
            "INSERT INTO clipboard_items (content_type, text_content, content_hash, semantic_hash)
                 VALUES ('text', 'preserved', 'hash', 'hash')",
            [],
        )
        .unwrap();
    drop(db);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE clipboard_items RENAME TO clipboard_items_new;")
        .unwrap();
    drop(conn);

    let reopened = Database::new(path).unwrap();
    let read = reopened.read_connection();
    let read = read.lock();
    let text: String = read
        .query_row("SELECT text_content FROM clipboard_items", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(text, "preserved");
    let orphan_count: i64 = read
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'clipboard_items_new'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(orphan_count, 0);
}

#[test]
fn migration_10_adds_url_content_type_from_legacy_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");

    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
                "CREATE TABLE groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );
                CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT);
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash, preview
                ) VALUES
                    ('text', 'https://example.com', 'hash_url', NULL, 'https://example.com'),
                    ('text', 'plain note', 'hash_text', '', 'plain note');",
            )
            .unwrap();
    }

    let db = Database::new(path.clone()).unwrap();
    let conn = db.read_connection();
    let conn = conn.lock();

    let url_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(url_count, 1);

    let null_semantic: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE semantic_hash IS NULL OR semantic_hash = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(null_semantic, 0);
    let marker: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = '_migration_url_content_type'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marker, "done");
    drop(conn);
    db.write_connection()
            .lock()
            .execute(
                "UPDATE clipboard_items SET content_type = 'text', text_content = 'https://example.com/edited' WHERE id = 1",
                [],
            )
            .unwrap();
    drop(db);
    let reopened = Database::new(path).unwrap();
    let content_type: String = reopened
        .read_connection()
        .lock()
        .query_row(
            "SELECT content_type FROM clipboard_items WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(content_type, "text");
}

#[test]
fn migration_6_accepts_legacy_url_content_type_without_group_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");

    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT
                );
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, preview
                ) VALUES
                    ('url', 'https://example.com', 'hash_url', 'https://example.com'),
                    ('video', 'legacy', 'hash_video', 'legacy');",
        )
        .unwrap();
    }

    let db = Database::new(path).unwrap();
    let conn = db.read_connection();
    let conn = conn.lock();

    let has_group_id: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'group_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(has_group_id);

    let url_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(url_count, 1);

    let video_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'video'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(video_count, 1);
}

#[test]
fn content_hash_index_migration_is_stable_after_reopening() {
    for state in ["current", "unique", "missing"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexes.db");
        let db = Database::new(path.clone()).unwrap();
        {
            let conn = db.write_connection();
            let conn = conn.lock();
            if state != "current" {
                conn.execute_batch(
                    "DROP INDEX idx_clipboard_hash_default;
                     DROP INDEX idx_clipboard_hash_group;",
                )
                .unwrap();
            }
            if state == "unique" {
                conn.execute_batch(
                    "CREATE UNIQUE INDEX idx_clipboard_hash_default
                       ON clipboard_items(content_hash) WHERE group_id IS NULL;
                     CREATE UNIQUE INDEX idx_clipboard_hash_group
                       ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;",
                )
                .unwrap();
            }
        }
        drop(db);

        let db = Database::new(path.clone()).unwrap();
        let schema_version: i64 = {
            let conn = db.write_connection();
            let conn = conn.lock();
            let indexes: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_index_list('clipboard_items')
                     WHERE name IN ('idx_clipboard_hash_default', 'idx_clipboard_hash_group')
                       AND \"unique\" = 0",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(indexes, 2, "{state}");
            conn.execute_batch(
                "INSERT INTO groups (id, name) VALUES (1, 'group');
                 INSERT INTO clipboard_items (content_type, content_hash, semantic_hash, group_id)
                 VALUES ('text', 'same', 'same', NULL), ('text', 'same', 'same', NULL),
                        ('text', 'same', 'same', 1), ('text', 'same', 'same', 1);",
            )
            .unwrap();
            conn.query_row("PRAGMA schema_version", [], |row| row.get(0))
                .unwrap()
        };
        drop(db);

        let reopened = Database::new(path).unwrap();
        let conn = reopened.read_connection();
        let conn = conn.lock();
        let reopened_version: i64 = conn
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(reopened_version, schema_version, "{state}");
        let duplicates: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_hash = 'same'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(duplicates, 4, "{state}");
    }
}
