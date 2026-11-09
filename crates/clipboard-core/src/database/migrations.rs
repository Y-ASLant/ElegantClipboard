use super::Database;
use rusqlite::Connection;
use tracing::info;

const V1_2_10_COLUMNS: &[&str] = &[
    "content_type",
    "text_content",
    "html_content",
    "rtf_content",
    "image_path",
    "file_paths",
    "file_payload",
    "content_hash",
    "semantic_hash",
    "preview",
    "byte_size",
    "image_width",
    "image_height",
    "is_pinned",
    "is_favorite",
    "favorite_order",
    "sort_order",
    "created_at",
    "updated_at",
    "access_count",
    "last_accessed_at",
    "char_count",
    "source_app_name",
    "source_app_icon",
    "group_id",
];

const V2_TABLES: &[&str] = &["native_formats", "captured_formats", "content_hash_aliases"];

impl Database {
    /// 只给 1.2.10 补 `note`。那一版已有当前列，且没有 2.0 的表。
    /// 更早的 1.x、其他 2.0 beta，以及已有 `note` 但缺这些列的库，拒绝打开且不改文件。
    /// 已有 `note` 和这些列的当前库不再做结构升级；缺 2.0 表时由后续 schema 补上。
    pub(super) fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        if !table_exists(conn, "clipboard_items")? {
            return Ok(());
        }
        for name in V1_2_10_COLUMNS {
            if !column_exists(conn, name)? {
                return Err(unsupported_database());
            }
        }
        if column_exists(conn, "note")? {
            return Ok(());
        }
        for name in V2_TABLES {
            if table_exists(conn, name)? {
                return Err(unsupported_database());
            }
        }
        info!("Migrating 1.2.10 database: adding note column");
        conn.execute_batch("ALTER TABLE clipboard_items ADD COLUMN note TEXT;")?;
        Ok(())
    }
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )
}

fn column_exists(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = ?1",
        [name],
        |row| row.get(0),
    )
}

fn unsupported_database() -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
        Some("数据库不是 1.2.10 或当前版本，不会升级".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::Database;
    use rusqlite::Connection;

    const V1_2_10: &str = "
        CREATE TABLE clipboard_items (
            id INTEGER PRIMARY KEY,
            content_type TEXT,
            text_content TEXT,
            html_content TEXT,
            rtf_content TEXT,
            image_path TEXT,
            file_paths TEXT,
            file_payload TEXT,
            content_hash TEXT,
            semantic_hash TEXT,
            preview TEXT,
            byte_size INTEGER,
            image_width INTEGER,
            image_height INTEGER,
            is_pinned INTEGER,
            is_favorite INTEGER,
            favorite_order INTEGER,
            sort_order INTEGER,
            created_at TEXT,
            updated_at TEXT,
            access_count INTEGER,
            last_accessed_at TEXT,
            char_count INTEGER,
            source_app_name TEXT,
            source_app_icon TEXT,
            group_id INTEGER
        );";

    fn columns(path: &std::path::Path) -> Vec<String> {
        let connection = Connection::open(path).unwrap();
        connection
            .prepare("SELECT name FROM pragma_table_info('clipboard_items') ORDER BY cid")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn has_table(path: &std::path::Path, name: &str) -> bool {
        Connection::open(path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn latest_1x_gains_note_and_other_shapes_stay_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let latest = directory.path().join("latest.db");
        Connection::open(&latest)
            .unwrap()
            .execute_batch(V1_2_10)
            .unwrap();
        Database::new(latest.clone()).unwrap();
        assert!(columns(&latest).contains(&"note".to_string()));
        assert!(has_table(&latest, "native_formats"));
        Database::new(latest.clone()).unwrap();

        let older = directory.path().join("older.db");
        Connection::open(&older)
            .unwrap()
            .execute_batch(
                "CREATE TABLE clipboard_items (id INTEGER PRIMARY KEY, content_type TEXT);",
            )
            .unwrap();
        assert!(Database::new(older.clone()).is_err());
        assert_eq!(
            columns(&older),
            vec!["id".to_string(), "content_type".to_string()]
        );
        assert!(!has_table(&older, "native_formats"));

        let beta = directory.path().join("beta.db");
        let connection = Connection::open(&beta).unwrap();
        connection.execute_batch(V1_2_10).unwrap();
        connection
            .execute_batch("CREATE TABLE native_formats (item_id INTEGER, format_name TEXT);")
            .unwrap();
        drop(connection);
        assert!(Database::new(beta.clone()).is_err());
        assert!(!columns(&beta).contains(&"note".to_string()));
        assert!(!has_table(&beta, "captured_formats"));
    }

    /// 1.2.10 升上来后仍可能留着 content_type CHECK；回填 URL 前要先去掉它。
    #[test]
    fn latest_1x_check_constraint_gets_url_and_semantic_backfill() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("latest_check.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'url')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    file_payload TEXT,
                    content_hash TEXT,
                    semantic_hash TEXT,
                    preview TEXT,
                    byte_size INTEGER,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER,
                    is_favorite INTEGER,
                    favorite_order INTEGER,
                    sort_order INTEGER,
                    created_at TEXT,
                    updated_at TEXT,
                    access_count INTEGER,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER
                );
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash, preview
                ) VALUES
                    ('text', 'https://example.com/page', 'hash_u', NULL, 'x'),
                    ('text', 'hello', 'hash_t', NULL, 'y');",
            )
            .unwrap();
        Database::new(path.clone()).unwrap();
        let conn = Connection::open(&path).unwrap();
        let url_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(url_rows, 1);
        assert!(columns(&path).contains(&"file_payload".to_string()));
        assert!(columns(&path).contains(&"note".to_string()));
        let sem: String = conn
            .query_row(
                "SELECT semantic_hash FROM clipboard_items WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sem, "hash_u");
        let marker: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = '_migration_url_content_type'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(marker, "done");
        let backfill: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = '_migration_backfill_semantic_hash'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(backfill, "done");
    }

    /// 已有备注但缺 `file_payload` 的 2.0 beta 不是当前库，也不能当作 1.2.10。
    #[test]
    fn note_without_file_payload_is_rejected_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note_without_payload.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT,
                    semantic_hash TEXT,
                    preview TEXT,
                    byte_size INTEGER,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER,
                    is_favorite INTEGER,
                    favorite_order INTEGER,
                    sort_order INTEGER,
                    created_at TEXT,
                    updated_at TEXT,
                    access_count INTEGER,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    note TEXT,
                    group_id INTEGER
                );
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash, preview, note
                ) VALUES ('text', 'https://example.com/plain', 'hash_p', NULL, 'x', 'keep');",
            )
            .unwrap();
        assert!(Database::new(path.clone()).is_err());
        assert!(!columns(&path).contains(&"file_payload".to_string()));
        assert!(!has_table(&path, "native_formats"));
        let conn = Connection::open(&path).unwrap();
        let row: (String, String) = conn
            .query_row(
                "SELECT content_type, note FROM clipboard_items",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, ("text".to_string(), "keep".to_string()));
    }
}
