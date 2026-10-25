use crate::History;
use anyhow::Result;

#[test]
fn plain_text_then_rich_merges_into_one_item() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let plain = history.capture("shared text")?.unwrap();
    let rich = history.capture_rich(
        Some("<b>shared text</b>"),
        None,
        Some("shared text"),
        &images,
    )?;
    assert_eq!(plain, rich, "富文本应并入既有同文条目");
    assert_eq!(history.count("", false)?, 1);
    let item = history.item(plain)?;
    assert_eq!(item.content_type, "html");
    assert_eq!(item.html_content.as_deref(), Some("<b>shared text</b>"));
    assert_eq!(item.text_content.as_deref(), Some("shared text"));
    Ok(())
}

#[test]
fn merged_alias_touches_canonical_on_next_capture() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let id = history.capture("alias text")?.unwrap();
    let rich =
        history.capture_rich(Some("<i>alias text</i>"), None, Some("alias text"), &images)?;
    assert_eq!(id, rich);
    // 第三次富文本采集：直接哈希（html:...）未命中，但别名应解析回规范条目。
    let third =
        history.capture_rich(Some("<i>alias text</i>"), None, Some("alias text"), &images)?;
    assert_eq!(third, id);
    assert_eq!(history.count("", false)?, 1);
    let item = history.item(id)?;
    assert_eq!(
        item.access_count, 2,
        "第三次采集经别名/直哈希命中后累计两次触达"
    );
    Ok(())
}

#[test]
fn url_target_keeps_type_but_rich_content_is_attached() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let url = history.capture("https://example.com/page")?.unwrap();
    assert_eq!(history.item(url)?.content_type, "url");
    let rich = history.capture_rich(
        Some("<a>https://example.com/page</a>"),
        None,
        Some("https://example.com/page"),
        &images,
    )?;
    assert_eq!(rich, url);
    assert_eq!(history.item(url)?.content_type, "url", "url 目标不升级类型");
    assert!(history.item(url)?.html_content.is_some());
    Ok(())
}

#[test]
fn rich_target_never_downgrades_and_html_stays_html() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let first = history.capture_rich(Some("<b>first</b>"), None, Some("body"), &images)?;
    // 相同文本再次以 RTF 复制：并入 html 条目并补 RTF 列，类型保持 html。
    let second = history.capture_rich(None, Some(b"{\\rtf1 body}"), Some("body"), &images)?;
    assert_eq!(first, second);
    let item = history.item(first)?;
    assert_eq!(item.content_type, "html");
    assert!(item.rtf_content.is_some());
    Ok(())
}

#[test]
fn custom_group_items_do_not_merge() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let group = history.create_group("工作")?;
    let id = history.capture("grouped text")?.unwrap();
    history.move_to_group(id, None, Some(group.id))?;
    // 默认分组内没有同文条目：不合并，新建。
    let second = history.capture("grouped text")?.unwrap();
    assert_ne!(id, second);
    // 默认分组只有新建的那条；原条目在自定义分组里。
    assert_eq!(history.count("", false)?, 1);
    assert_eq!(history.count_in_group("", false, Some(group.id))?, 1);
    Ok(())
}

#[test]
fn deleted_target_leaves_stale_alias_that_recreates_new_item() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let id = history.capture("doomed")?.unwrap();
    let _ = history.capture_rich(Some("<b>doomed</b>"), None, Some("doomed"), &images)?;
    history.delete_with_media(id, &images)?;
    // 别名残留指向已删除条目：touch 解析失败，应创建新条目而非报错。
    let recreated = history.capture("doomed")?.unwrap();
    assert_eq!(history.count("", false)?, 1);
    let _ = history.capture_rich(Some("<b>doomed</b>"), None, Some("doomed"), &images)?;
    assert_eq!(
        history.capture_rich(Some("<b>doomed</b>"), None, Some("doomed"), &images)?,
        recreated
    );
    Ok(())
}

#[test]
fn merged_preview_and_byte_size_are_recomputed() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let id = history.capture("merge size")?.unwrap();
    let original_size = history.item(id)?.byte_size;
    let html = "<b>merge size</b>";
    let _ = history.capture_rich(Some(html), None, Some("merge size"), &images)?;
    let item = history.item(id)?;
    assert_eq!(
        item.byte_size,
        original_size + html.len() as i64,
        "byte_size 应重算为 text+html+rtf 总和"
    );
    assert_eq!(item.preview.as_deref(), Some("merge size"));
    Ok(())
}
