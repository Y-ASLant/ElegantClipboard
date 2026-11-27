use super::Database;
use rusqlite::{Connection, Transaction};
use tracing::info;

const V1_2_10_COLUMNS: &[&str] = &[
    "id",
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

impl Database {
    /// 校验旧库形状并在创建当前索引前升级列。返回是否从 1.2.10 补上了 `note`。
    pub(super) fn run_migrations(conn: &Transaction<'_>) -> Result<bool, rusqlite::Error> {
        let columns = clipboard_item_columns(conn)?;
        if columns.is_empty() {
            let empty: bool = conn.query_row(
                "SELECT NOT EXISTS(SELECT 1 FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*')",
                [],
                |row| row.get(0),
            )?;
            return if empty {
                Ok(false)
            } else {
                Err(unsupported_database("缺少历史表"))
            };
        }
        let missing: Vec<&str> = V1_2_10_COLUMNS
            .iter()
            .copied()
            .filter(|name| !columns.contains(*name))
            .collect();
        if !missing.is_empty() {
            return Err(unsupported_database(&format!(
                "缺少列：{}",
                missing.join(", ")
            )));
        }
        if ["note", "is_favorite_pinned", "categories"]
            .iter()
            .all(|name| columns.contains(*name))
        {
            return Ok(false);
        }
        // 只从发布的 1.2.10 迁移；不尝试修复缺列的 2.0 开发库。
        if columns.len() != V1_2_10_COLUMNS.len() {
            return Err(unsupported_database("历史表列不匹配"));
        }
        let legacy_tables: bool = conn.query_row(
            "SELECT COUNT(*) = 3 AND COUNT(*) FILTER (
                 WHERE name IN ('clipboard_items', 'settings', 'groups')) = 3
             FROM sqlite_schema WHERE type = 'table' AND name NOT GLOB 'sqlite_*'",
            [],
            |row| row.get(0),
        )?;
        if !legacy_tables {
            return Err(unsupported_database("不是 1.2.10 的表结构"));
        }
        info!("Migrating 1.2.10 clipboard metadata");
        // 1.2.10 has a timestamp trigger but no FTS or favorite list indexes.
        // SCHEMA_SQL reinstalls the trigger in the same transaction.
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS clipboard_items_update_timestamp;
             ALTER TABLE clipboard_items ADD COLUMN note TEXT;
             ALTER TABLE clipboard_items ADD COLUMN categories INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE clipboard_items ADD COLUMN is_favorite_pinned INTEGER NOT NULL DEFAULT 0;
             UPDATE clipboard_items SET is_favorite_pinned = 1
             WHERE is_favorite = 1 AND is_pinned != 0;",
        )?;
        Ok(true)
    }
}

fn clipboard_item_columns(
    conn: &Connection,
) -> Result<std::collections::HashSet<String>, rusqlite::Error> {
    // 一次性读出列名，避免 pragma_table_info 绑定参数在部分 SQLite 上不稳定。
    let mut statement = conn.prepare("PRAGMA table_info(clipboard_items)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    Ok(columns)
}

fn unsupported_database(detail: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
        Some(format!(
            "数据库不是 1.2.10 或当前版本，不会升级（{detail}）"
        )),
    )
}

#[cfg(test)]
mod tests {
    use crate::database::{Database, V1_2_10_SCHEMA};
    use rusqlite::Connection;

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
            .execute_batch(V1_2_10_SCHEMA)
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

        let hybrid = directory.path().join("hybrid.db");
        let connection = Connection::open(&hybrid).unwrap();
        connection.execute_batch(V1_2_10_SCHEMA).unwrap();
        connection
            .execute_batch("CREATE TABLE native_formats (item_id INTEGER, format_name TEXT);")
            .unwrap();
        drop(connection);
        assert!(Database::new(hybrid.clone()).is_err());
        assert!(!columns(&hybrid).contains(&"note".to_string()));
        assert!(!has_table(&hybrid, "captured_formats"));
    }

    #[test]
    fn shared_pin_upgrades_only_favorites_and_never_reseeds_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(V1_2_10_SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO clipboard_items (id, content_type, content_hash, semantic_hash,
                                          is_pinned, is_favorite, favorite_order) VALUES
                 (1, 'text', 'a', 'a', 1, 1, 2), (2, 'text', 'b', 'b', 0, 1, 1),
                 (3, 'text', 'c', 'c', 1, 0, 0), (4, 'text', 'd', 'd', 0, 0, 0);",
        )
        .unwrap();
        let pins = || {
            conn.prepare("SELECT is_pinned, is_favorite_pinned FROM clipboard_items ORDER BY id")
                .unwrap()
                .query_map([], |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        Database::new(path.clone()).unwrap();
        assert_eq!(
            pins(),
            [(true, true), (false, false), (true, false), (false, false)]
        );
        for (index, position) in [
            ("idx_clipboard_favorite_default_list", 0),
            ("idx_clipboard_favorite_group_list", 1),
        ] {
            let column: String = conn
                .query_row(
                    "SELECT name FROM pragma_index_info(?1) WHERE seqno = ?2",
                    rusqlite::params![index, position],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(column, "is_favorite_pinned");
        }
        conn.execute(
            "UPDATE clipboard_items SET is_favorite_pinned = NOT is_favorite_pinned WHERE is_favorite = 1",
            [],
        )
        .unwrap();
        Database::new(path).unwrap();
        assert_eq!(
            pins(),
            [(true, false), (false, true), (true, false), (false, false)]
        );
    }

    #[test]
    fn interrupted_pin_backfill_does_not_commit_its_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interrupted.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(V1_2_10_SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO clipboard_items (content_type, content_hash, semantic_hash, is_pinned, is_favorite)
             VALUES ('text', 'a', 'a', 1, 1);
             CREATE TRIGGER fail_backfill BEFORE UPDATE ON clipboard_items
             BEGIN SELECT RAISE(ABORT, 'interrupted'); END;",
        )
        .unwrap();
        assert!(Database::new(path.clone()).is_err());
        assert!(!columns(&path).iter().any(|c| c == "is_favorite_pinned"));
        conn.execute_batch("DROP TRIGGER fail_backfill").unwrap();
        Database::new(path).unwrap();
        let pin: bool = conn
            .query_row("SELECT is_favorite_pinned FROM clipboard_items", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(pin);
    }

    /// 1.2.10 升上来后仍可能留着 content_type CHECK；回填 URL 前要先去掉它。
    #[test]
    fn latest_1x_check_constraint_gets_url_and_semantic_backfill() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("latest_check.db");
        let legacy = V1_2_10_SCHEMA
            .replace(
                "content_type TEXT NOT NULL,",
                "content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'url')),",
            )
            .replace("semantic_hash TEXT NOT NULL,", "semantic_hash TEXT,");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&legacy).unwrap();
        conn.execute_batch(
            "INSERT INTO clipboard_items (
                content_type, text_content, content_hash, semantic_hash, preview, is_pinned, is_favorite
             ) VALUES
                ('text', 'https://example.com/page', 'hash_u', NULL, 'x', 1, 1),
                ('text', 'hello', 'hash_t', NULL, 'y', 1, 0);",
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
        let pinned: i64 = conn
            .query_row(
                "SELECT id FROM clipboard_items WHERE is_favorite_pinned = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pinned, 1);
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

    /// 有 `note` 但缺 1.2.10 列（如 `file_payload`）的库既非当前也非 1.2.10。
    #[test]
    fn incomplete_schema_with_note_is_rejected_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("incomplete_note.db");
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
