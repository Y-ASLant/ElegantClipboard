use super::*;

#[test]
fn invalid_json_and_path_shapes_roll_back_earlier_rows_and_allow_retry() -> Result<()> {
    for (column, invalid) in [
        ("file_paths", "[broken"),
        ("file_payload", "[broken"),
        ("file_paths", "{}"),
        ("file_payload", r#"{"staged": {}}"#),
        ("file_payload", r#"{"staged": [{"original": []}]}"#),
    ] {
        let fixture = Fixture::new()?;
        let managed = fixture.roots[0].join("staged/retry.txt");
        let valid = if column == "file_paths" {
            json!([managed]).to_string()
        } else {
            json!({"staged": [{"original": fixture.outside, "staged": managed}]}).to_string()
        };
        let malformed = if invalid == "[broken" {
            // 第一条记录已修改，后续记录失败须连同前面的修改一起回滚。
            format!("[{},", json!(managed))
        } else {
            invalid.to_owned()
        };
        fixture.connection()?.execute(
            &format!("UPDATE clipboard_items SET {column} = ?1 WHERE id = 42"),
            [malformed],
        )?;
        let before = snapshot(&fixture.connection()?)?;
        assert!(fixture.relocate().is_err(), "{column}: {invalid}");
        fixture.assert_unlocked()?;
        assert_eq!(
            snapshot(&fixture.connection()?)?,
            before,
            "{column}: {invalid}"
        );
        fixture.connection()?.execute(
            &format!("UPDATE clipboard_items SET {column} = ?1 WHERE id = 42"),
            [valid],
        )?;
        fixture.relocate()?;
        let raw: String = fixture.connection()?.query_row(
            &format!("SELECT {column} FROM clipboard_items WHERE id = 42"),
            [],
            |row| row.get(0),
        )?;
        let value: Value = serde_json::from_str(&raw)?;
        let path = if column == "file_paths" {
            &value[0]
        } else {
            &value["staged"][0]["staged"]
        };
        assert_eq!(
            path,
            &json!(fs::canonicalize(&fixture.target)?.join("staged/retry.txt"))
        );
    }
    Ok(())
}

#[test]
fn late_sql_failure_rolls_back_paths_and_trigger_and_can_be_retried() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.connection()?.execute_batch(
        "CREATE TRIGGER reject_second BEFORE UPDATE ON clipboard_items
         WHEN old.id = 41
         BEGIN SELECT RAISE(ABORT, 'injected later-row failure'); END;",
    )?;
    let earlier = fixture.roots[0].join("images/earlier.png");
    fixture.connection()?.execute(
        "INSERT INTO clipboard_items (id, content_type, image_path, content_hash,
            semantic_hash, created_at, updated_at)
         VALUES (40, 'image', ?1, 'earlier', 'earlier',
            '2001-01-01 01:01:01', '2002-02-02 02:02:02')",
        [earlier.to_str()],
    )?;
    let before = snapshot(&fixture.connection()?)?;
    assert!(fixture.relocate().is_err());
    fixture.assert_unlocked()?;
    assert_eq!(snapshot(&fixture.connection()?)?, before);
    fixture
        .connection()?
        .execute_batch("DROP TRIGGER reject_second;")?;
    fixture.relocate()?;
    let mapped: String = fixture.connection()?.query_row(
        "SELECT image_path FROM clipboard_items WHERE id = 40",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        mapped,
        fs::canonicalize(&fixture.target)?
            .join("images/earlier.png")
            .to_str()
            .unwrap()
    );
    Ok(())
}

#[test]
fn overly_nested_payload_returns_an_error_and_rolls_back_path_changes() -> Result<()> {
    let fixture = Fixture::new()?;
    let deeply_nested = format!(
        "{{\"staged\":[],\"unknown\":{}0{}}}",
        "[".repeat(256),
        "]".repeat(256)
    );
    fixture.connection()?.execute(
        "UPDATE clipboard_items SET file_payload = ?1 WHERE id = 42",
        [deeply_nested],
    )?;
    let before = snapshot(&fixture.connection()?)?;
    assert!(fixture.relocate().is_err());
    assert_eq!(snapshot(&fixture.connection()?)?, before);
    fixture.assert_unlocked()?;
    Ok(())
}

#[test]
fn invalid_roots_and_non_history_input_leave_existing_data_untouched() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = snapshot(&fixture.connection()?)?;
    for root in [
        PathBuf::from("relative-root"),
        fixture.target.ancestors().last().unwrap().to_path_buf(),
    ] {
        assert!(relocate_managed_paths(&fixture.target, &[root]).is_err());
        assert_eq!(snapshot(&fixture.connection()?)?, before);
    }
    assert!(relocate_managed_paths(Path::new("relative-target"), &fixture.roots).is_err());
    let missing = tempfile::tempdir()?;
    assert!(relocate_managed_paths(missing.path(), &fixture.roots).is_err());
    assert!(!missing.path().join("clipboard.db").exists());
    let invalid = missing.path().join("clipboard.db");
    fs::write(&invalid, b"not sqlite")?;
    assert!(relocate_managed_paths(missing.path(), &fixture.roots).is_err());
    assert_eq!(fs::read(&invalid)?, b"not sqlite");
    fs::remove_file(&invalid)?;
    Connection::open(&invalid)?.execute_batch("CREATE TABLE other (value TEXT);")?;
    assert!(relocate_managed_paths(missing.path(), &fixture.roots).is_err());
    let has_history: bool = Connection::open(&invalid)?.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'clipboard_items')",
        [],
        |row| row.get(0),
    )?;
    assert!(!has_history);
    Ok(())
}

#[test]
fn lexical_boundaries_parent_components_and_nested_destination_are_stable() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = &fixture.roots[0];
    let relocation = Relocation::new(&fixture.target, &fixture.roots)?;
    assert_eq!(
        relocation.map(root.join("images/../staged/file.txt").to_str().unwrap()),
        Some(
            fs::canonicalize(&fixture.target)?
                .join("staged/file.txt")
                .to_str()
                .unwrap()
                .to_owned()
        )
    );
    assert_eq!(
        relocation.map(root.join("../outside.txt").to_str().unwrap()),
        None
    );
    assert_eq!(
        relocation.map(&format!("{}-other/file.txt", root.display())),
        None
    );
    assert_eq!(relocation.map("relative/file.txt"), None);
    let parent_root = fixture.target.parent().unwrap().to_path_buf();
    let nested = Relocation::new(&fixture.target, &[parent_root])?;
    assert_eq!(
        nested.map(fixture.target.join("images/new.png").to_str().unwrap()),
        None
    );
    #[cfg(not(windows))]
    assert_eq!(
        relocation.map(&root.join("images/a.png").to_str().unwrap().to_uppercase()),
        None
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn windows_verbatim_case_and_separator_variants_match_raw_roots() -> Result<()> {
    let fixture = Fixture::new()?;
    let root = fixture.roots[0].to_str().unwrap();
    let paths = json!([
        format!(r"{}\staged\raw.txt", root.to_uppercase()),
        format!(r"\\?\{}\staged\verbatim.txt", root.to_lowercase()),
        format!("{}/staged/slashes.txt", root.replace('\\', "/")),
        format!(r"{}-other\staged\external.txt", root),
        format!(r"{}\..\outside.txt", root),
        format!(r"{}\staged\..\images\normalized.png", root),
        format!(r"\\?\{}\staged\..\images\verbatim-normalized.png", root),
        format!(r"\\?\{}\..\outside-verbatim.txt", root),
    ]);
    fixture.connection()?.execute(
        "UPDATE clipboard_items SET file_paths = ?1 WHERE id = 41",
        [paths.to_string()],
    )?;
    relocate_managed_paths(&fixture.target, &[fixture.roots[0].clone()])?;
    let raw: String = fixture.connection()?.query_row(
        "SELECT file_paths FROM clipboard_items WHERE id = 41",
        [],
        |row| row.get(0),
    )?;
    let mapped: Value = serde_json::from_str(&raw)?;
    let destination = fs::canonicalize(&fixture.target)?;
    for (index, relative) in [
        "staged/raw.txt",
        "staged/verbatim.txt",
        "staged/slashes.txt",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(mapped[index], json!(destination.join(relative)));
    }
    assert_eq!(mapped[3], paths[3]);
    assert_eq!(mapped[4], paths[4]);
    assert_eq!(mapped[5], json!(destination.join("images/normalized.png")));
    assert_eq!(
        mapped[6],
        json!(destination.join("images/verbatim-normalized.png"))
    );
    assert_eq!(mapped[7], paths[7]);
    let unc = Relocation::new(&fixture.target, &[PathBuf::from(r"\\server\share\old")])?;
    assert_eq!(
        unc.map(r"\\?\UNC\SERVER\SHARE\old\staged\copy.txt"),
        Some(
            destination
                .join("staged/copy.txt")
                .to_str()
                .unwrap()
                .to_owned()
        )
    );
    Ok(())
}
