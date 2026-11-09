use super::*;

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
