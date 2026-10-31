use super::*;
use clipboard_core::{
    PreviewContent,
    database::{ClipboardRepository, ContentType, NewClipboardItem, QueryOptions},
    search::search_highlight_ranges,
};

#[test]
fn search_snapshots_keep_context_without_retaining_bodies_or_changing_full_reads() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("clipboard.db");
    let db = Database::new(path.clone())?;
    let repo = ClipboardRepository::new(&db);
    let term = "AbC目标🙂";
    let search = "aBc目标🙂";
    let deep = format!("{}{term}{}", "前🙂".repeat(150), "后".repeat(120));
    // A short body can still need an excerpt when the saved preview is shorter.
    let short = format!("前缀 {term} 尾部");
    let saved = format!("保留 {term}");
    let expected_deep = format!("…{}{term}{}…", "前🙂".repeat(10), "后".repeat(80));
    let fixtures = [
        (
            deep.clone(),
            Some("前🙂前🙂".to_owned()),
            expected_deep.clone(),
        ),
        (short.clone(), Some("前缀".to_owned()), short.clone()),
        (format!("{term} 完整正文"), Some(saved.clone()), saved),
        (deep.clone(), None, expected_deep),
    ];
    let mut ids = Vec::new();
    for (index, (full, preview, _)) in fixtures.iter().enumerate() {
        ids.push(repo.insert(NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(full.clone()),
            preview: preview.clone(),
            content_hash: format!("search-context-{index}"),
            semantic_hash: format!("search-context-{index}"),
            byte_size: full.len() as i64,
            char_count: Some(full.chars().count() as i64),
            ..Default::default()
        })?);
    }
    let (service, events) = Service::start(Some(directory.path().to_owned()), false)?;
    next_snapshot(&events, 0);
    service.send(Command::Query {
        search: search.into(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 1,
    })?;
    let rows = next_snapshot(&events, 1);
    assert_eq!(rows.len(), fixtures.len());
    let full_rows = repo.list(QueryOptions {
        search: Some(search.into()),
        ..Default::default()
    })?;
    for (&id, (full, preview, expected)) in ids.iter().zip(&fixtures) {
        let row = rows.iter().find(|row| row.id == id).unwrap();
        assert!(row.text_content.is_none());
        assert_eq!(row.preview.as_deref(), Some(expected.as_str()));
        assert_eq!(row.byte_size, full.len() as i64);
        let matches = search_highlight_ranges(expected, search);
        assert_eq!(matches.len(), 1);
        assert_eq!(&expected[matches[0].clone()], term);
        let listed = full_rows.iter().find(|row| row.id == id).unwrap();
        assert_eq!(listed.text_content.as_deref(), Some(full.as_str()));
        let stored = repo.get_by_id(id)?.unwrap();
        assert_eq!(stored.text_content.as_deref(), Some(full.as_str()));
        assert_eq!(&stored.preview, preview);
    }
    // The copy path reads by ID too; this checks its full-content source without
    // sending any command that writes to the user's clipboard.
    assert_eq!(History::open(path)?.text(ids[0])?, deep);
    service.send(Command::Preview {
        id: ids[0],
        generation: 2,
    })?;
    assert!(matches!(
        next_event(&events),
        Event::Preview { id, generation: 2, result: Ok(PreviewContent::Text(text)) }
            if id == ids[0] && text == deep
    ));
    service.send(Command::HoverPreview {
        id: ids[1],
        generation: 3,
    })?;
    assert!(matches!(
        next_event(&events),
        Event::HoverPreview { id, generation: 3, result: Ok(PreviewContent::Text(text)) }
            if id == ids[1] && text == short
    ));
    service.send(Command::Query {
        search: String::new(),
        favorite_only: false,
        category: ContentCategory::All,
        group_id: None,
        limit: PAGE_SIZE,
        generation: 4,
    })?;
    let rows = next_snapshot(&events, 4);
    for (&id, (_, preview, _)) in ids.iter().zip(&fixtures) {
        let row = rows.iter().find(|row| row.id == id).unwrap();
        assert!(row.text_content.is_none());
        assert_eq!(&row.preview, preview);
    }
    Ok(())
}
