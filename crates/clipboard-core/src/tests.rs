use super::*;
use preferences::DedupStrategy;

#[test]
fn always_new_strategy_keeps_duplicate_copies_and_move_to_top_merges() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    // 默认策略：同哈希移顶合并。
    let first = history.capture("repeat")?.unwrap();
    assert_eq!(history.capture("repeat")?.unwrap(), first);
    assert_eq!(history.count("", false)?, 1);
    // always_new：同哈希也各存一条；文本与富文本路径都受策略控制。
    let preferences = Preferences::new(&history.db);
    preferences.set_dedup_strategy(DedupStrategy::AlwaysNew)?;
    let second = history.capture("repeat")?.unwrap();
    assert_ne!(second, first);
    let rich_second = history.capture_rich(
        Some("<b>repeat</b>"),
        None,
        Some("repeat"),
        &dir.path().join("images"),
    )?;
    assert_ne!(rich_second, first);
    assert_eq!(history.count("", false)?, 3);
    // 切回默认策略后恢复合并，且 always_new 期间的条目仍可被触碰。
    preferences.set_dedup_strategy(DedupStrategy::MoveToTop)?;
    assert_eq!(history.capture("repeat")?.unwrap(), second);
    assert_eq!(history.count("", false)?, 3);
    Ok(())
}

#[test]
fn notes_persist_search_and_reject_oversize() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let id = history.capture("noted body")?.unwrap();
    assert!(history.item(id)?.note.is_none());
    history.set_note(id, "  发布说明草稿  ")?;
    assert_eq!(history.item(id)?.note.as_deref(), Some("发布说明草稿"));
    // 备注参与关键词搜索。
    assert_eq!(history.count("发布说明", false)?, 1);
    assert_eq!(history.count("不存在的备注", false)?, 0);
    // 空串清空；超限拒绝且不改动原值。
    history.set_note(id, "   ")?;
    assert_eq!(history.item(id)?.note, None);
    let oversize = "字".repeat(MAX_NOTE_CHARS + 1);
    assert!(history.set_note(id, &oversize).is_err());
    assert!(history.set_note(999_999, "任意").is_err());
    Ok(())
}

#[test]
fn text_transforms_cover_the_four_special_paste_modes() {
    assert_eq!(TextTransform::Trim.apply("  Hello World  "), "Hello World");
    assert_eq!(
        TextTransform::RemoveLineBreaks.apply("  Hello\r\nWorld  "),
        "Hello World"
    );
    assert_eq!(TextTransform::Uppercase.apply("abc"), "ABC");
    assert_eq!(TextTransform::Lowercase.apply("ABC"), "abc");
}

#[test]
fn database_maintenance_reclaims_free_pages_and_preserves_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("clipboard.db");
    let history = History::open(database.clone())?;
    let id = history.capture("keep after maintenance")?.unwrap();
    {
        let connection = history.db.write_connection();
        let connection = connection.lock();
        connection.execute_batch(
            "CREATE TABLE maintenance_fixture (payload BLOB NOT NULL);
                 INSERT INTO maintenance_fixture(payload) VALUES (zeroblob(4194304));
                 DELETE FROM maintenance_fixture;
                 PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
    }
    let before = std::fs::metadata(&database)?.len();

    history.optimize_storage()?;

    let after = std::fs::metadata(&database)?.len();
    assert!(after < before, "before={before}, after={after}");
    assert_eq!(history.text(id)?, "keep after maintenance");
    Ok(())
}

#[test]
fn group_creation_validates_names_and_persists() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    assert!(history.create_group(" \n ").is_err());
    assert!(history.create_group(&"名".repeat(41)).is_err());
    let group = history.create_group("  工作  ")?;
    assert_eq!(group.name, "工作");
    assert!(history.create_group("工作").is_err());
    drop(history);
    assert_eq!(History::open(path)?.groups()?[0].id, group.id);
    Ok(())
}

#[test]
fn group_rename_preserves_membership_and_rejects_duplicates() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let work = history.create_group("工作")?;
    history.create_group("归档")?;
    let id = history.capture("work item")?.unwrap();
    history.move_to_group(id, None, Some(work.id))?;
    assert!(history.rename_group(work.id, "归档").is_err());
    assert!(history.rename_group(-1, "其他").is_err());
    assert!(history.rename_group(work.id, " ").is_err());
    let renamed = history.rename_group(work.id, "  常用  ")?;
    assert_eq!(renamed.name, "常用");
    assert_eq!(history.item(id)?.group_id, Some(work.id));
    drop(history);
    assert_eq!(History::open(path)?.groups()?[0].name, "常用");
    Ok(())
}

#[test]
fn moving_items_between_groups_preserves_history_and_rejects_stale_source() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let history = History::open(dir.path().join("clipboard.db"))?;
    let id = history.capture("important text")?.unwrap();
    history.toggle_favorite(id)?;
    history.toggle_pin(id)?;
    let work = history.create_group("工作")?;
    let archive = history.create_group("归档")?;
    assert!(history.move_to_group(id, None, Some(-1)).is_err());
    history.move_to_group(id, None, Some(work.id))?;
    assert!(history.list("", PAGE_SIZE, false)?.is_empty());
    assert_eq!(
        history.list_in_group("", PAGE_SIZE, false, Some(work.id))?[0].id,
        id
    );
    assert!(history.move_to_group(id, None, Some(archive.id)).is_err());
    history.move_to_group(id, Some(work.id), Some(archive.id))?;
    assert_eq!(history.count_in_group("", false, Some(work.id))?, 0);
    let item = history.item(id)?;
    assert_eq!(item.group_id, Some(archive.id));
    assert!(item.is_pinned && item.is_favorite);
    history.move_to_group(id, Some(archive.id), None)?;
    assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
    Ok(())
}

#[test]
fn favorites_persist_filter_search_and_keep_full_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let history = History::open(path.clone())?;
    let id = history.capture("收藏中文%_😀")?.unwrap();
    history.capture("普通中文%_😀")?;
    assert!(history.toggle_favorite(id)?);
    assert_eq!(history.count("%_", true)?, 1);
    assert_eq!(history.list("%_", PAGE_SIZE, true)?[0].id, id);
    assert_eq!(history.count("%_", false)?, 2);
    assert!(history.list("不存在", PAGE_SIZE, true)?.is_empty());
    drop(history);
    let history = History::open(path)?;
    assert_eq!(history.count("", true)?, 1);
    assert_eq!(history.capture("收藏中文%_😀")?, Some(id));
    assert_eq!(history.count("", true)?, 1);
    assert!(!history.toggle_favorite(id)?);
    assert!(history.list("", PAGE_SIZE, true)?.is_empty());
    assert_eq!(history.count("", false)?, 2);
    assert!(history.toggle_favorite(-1).is_err());
    Ok(())
}

#[test]
fn history_persists_deduplicates_and_preserves_full_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let text = format!("中文😀\r\n{}", "a".repeat(600));
    let id;
    {
        let history = History::open(path.clone())?;
        id = history.capture(&text)?.unwrap();
        history.capture("second")?;
        assert_eq!(history.capture(&text)?, Some(id));
        assert_eq!(history.count("", false)?, 2);
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
    }
    let history = History::open(path)?;
    assert_eq!(history.text(id)?, text);
    assert_eq!(history.count("中文😀", false)?, 1);
    Ok(())
}

#[test]
fn imported_urls_are_visible_and_keep_legacy_dedup_identity() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let url = "https://example.com/旧记录";
    let hash = blake3::hash(format!("url:{url}").as_bytes())
        .to_hex()
        .to_string();
    let id = ClipboardRepository::new(&db).insert(NewClipboardItem {
        content_type: ContentType::Url,
        text_content: Some(url.into()),
        content_hash: hash.clone(),
        semantic_hash: hash,
        preview: Some(url.into()),
        ..Default::default()
    })?;
    let history = History::new(&db);
    assert_eq!(history.count("example.com", false)?, 1);
    assert_eq!(history.list("旧记录", PAGE_SIZE, false)?[0].id, id);
    assert_eq!(history.capture(&format!("  {url}  "))?, Some(id));
    assert_eq!(history.count("", false)?, 1);
    assert_eq!(history.text(id)?, url);
    Ok(())
}

#[test]
fn category_queries_filter_search_count_and_group() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let db = Database::new(directory.path().join("clipboard.db"))?;
    let repo = ClipboardRepository::new(&db);
    let mut ids = Vec::new();
    for (index, kind) in [
        ContentType::Text,
        ContentType::Html,
        ContentType::Rtf,
        ContentType::Image,
        ContentType::Files,
        ContentType::Url,
    ]
    .into_iter()
    .enumerate()
    {
        ids.push(repo.insert(NewClipboardItem {
            content_type: kind,
            text_content: Some(format!("needle-{index}")),
            content_hash: format!("category-{index}"),
            semantic_hash: format!("category-{index}"),
            ..Default::default()
        })?);
    }
    let history = History::new(&db);
    assert_eq!(
        history.count_filtered_in_group("needle", false, None, ContentCategory::Text)?,
        3
    );
    assert_eq!(
        history
            .list_filtered_in_group("", PAGE_SIZE, false, None, ContentCategory::Other)?
            .len(),
        3
    );
    history.toggle_favorite(ids[1])?;
    assert_eq!(
        history.count_filtered_in_group("", true, None, ContentCategory::Text)?,
        1
    );
    let group = history.create_group("archive")?;
    history.move_to_group(ids[5], None, Some(group.id))?;
    assert_eq!(
        history.count_filtered_in_group("", false, None, ContentCategory::Other)?,
        2
    );
    assert_eq!(
        history.list_filtered_in_group(
            "needle",
            PAGE_SIZE,
            false,
            Some(group.id),
            ContentCategory::Other
        )?[0]
            .id,
        ids[5]
    );
    Ok(())
}

#[test]
fn literal_search_pin_delete_and_limits() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let id = history.capture("100%_done")?.unwrap();
    history.capture("plain text")?;
    assert_eq!(history.count("%_", false)?, 1);
    history.toggle_pin(id)?;
    assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, id);
    assert_eq!(history.capture(" \r\n")?, None);
    assert!(history.capture(&"x".repeat(MAX_TEXT_BYTES + 1)).is_err());
    history.delete(id)?;
    assert!(history.text(id).is_err());
    assert_eq!(history.count("", false)?, 1);
    Ok(())
}
