use super::*;

#[test]
fn partial_paired_columns_are_recovered_without_changing_existing_values() {
    for missing in [
        "image_width",
        "image_height",
        "source_app_name",
        "source_app_icon",
    ] {
        let (dir, db) = temp_db();
        let path = dir.path().join("test.db");
        {
            let conn = db.write_connection();
            let conn = conn.lock();
            conn.execute_batch(
                "INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash,
                    image_width, image_height, source_app_name, source_app_icon
                 ) VALUES ('text', '保留内容', 'hash', 'hash', 640, 480, 'editor', 'icon.png');",
            )
            .unwrap();
            conn.execute_batch(&format!(
                "ALTER TABLE clipboard_items DROP COLUMN {missing};"
            ))
            .unwrap();
        }
        drop(db);

        for _ in 0..2 {
            let db = Database::new(path.clone()).unwrap();
            let item = ClipboardRepository::new(&db).get_by_id(1).unwrap().unwrap();
            assert_eq!(item.text_content.as_deref(), Some("保留内容"));
            assert_eq!(item.image_width, (missing != "image_width").then_some(640));
            assert_eq!(
                item.image_height,
                (missing != "image_height").then_some(480)
            );
            assert_eq!(
                item.source_app_name.as_deref(),
                (missing != "source_app_name").then_some("editor")
            );
            assert_eq!(
                item.source_app_icon.as_deref(),
                (missing != "source_app_icon").then_some("icon.png")
            );
        }
    }
}

#[test]
fn failed_column_backfill_rolls_back_and_can_be_retried() {
    for column in ["sort_order", "char_count", "favorite_order"] {
        let (dir, db) = temp_db();
        let path = dir.path().join("test.db");
        {
            let conn = db.write_connection();
            let conn = conn.lock();
            conn.execute_batch(
                "INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash,
                    is_favorite, favorite_order, sort_order, char_count
                 ) VALUES
                    ('text', '中文甲', 'first', 'first', 1, 20, 20, 3),
                    ('text', '中文乙乙', 'second', 'second', 1, 10, 10, 4);",
            )
            .unwrap();
            if column == "sort_order" {
                conn.execute_batch("DROP INDEX idx_clipboard_sort_order;")
                    .unwrap();
            } else if column == "favorite_order" {
                conn.execute_batch("DROP INDEX idx_clipboard_favorite_order;")
                    .unwrap();
            }
            conn.execute_batch(&format!(
                "ALTER TABLE clipboard_items DROP COLUMN {column};
                 CREATE TRIGGER reject_backfill BEFORE UPDATE ON clipboard_items
                 BEGIN SELECT RAISE(ABORT, 'injected backfill failure'); END;"
            ))
            .unwrap();
        }
        drop(db);

        assert!(Database::new(path.clone()).is_err(), "{column}");
        {
            let conn = Connection::open(&path).unwrap();
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('clipboard_items') WHERE name = ?1)",
                [column],
                |row| row.get(0),
            ).unwrap();
            assert!(
                !exists,
                "failed {column} migration must roll back its ALTER"
            );
            let values: Vec<String> = conn
                .prepare("SELECT text_content FROM clipboard_items ORDER BY id")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(values, ["中文甲", "中文乙乙"]);
            conn.execute_batch("DROP TRIGGER reject_backfill;").unwrap();
        }

        for _ in 0..2 {
            let db = Database::new(path.clone()).unwrap();
            let repo = ClipboardRepository::new(&db);
            let first = repo.get_by_id(1).unwrap().unwrap();
            let second = repo.get_by_id(2).unwrap().unwrap();
            match column {
                "sort_order" => assert_eq!((first.sort_order, second.sort_order), (1, 2)),
                "char_count" => {
                    assert_eq!((first.char_count, second.char_count), (Some(3), Some(4)))
                }
                "favorite_order" => {
                    assert_eq!((first.favorite_order, second.favorite_order), (20, 10))
                }
                _ => unreachable!(),
            }
        }
    }
}
