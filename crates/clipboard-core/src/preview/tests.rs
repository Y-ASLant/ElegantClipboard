use super::*;
use crate::History;

#[test]
fn html_preview_prefers_start_fragment() {
    let html = "<html><body><!--StartFragment--><p>片段</p><!--EndFragment--></body></html>";
    assert_eq!(html_preview_fragment(html), "<p>片段</p>");
}

#[test]
fn html_preview_falls_back_to_full_source_without_markers() {
    assert_eq!(html_preview_fragment("<p>整段</p>"), "<p>整段</p>");
}

#[test]
fn html_preview_is_case_insensitive_and_rejects_empty_fragment() {
    let lower = "<!--startfragment--><b>x</b><!--endfragment-->";
    assert_eq!(html_preview_fragment(lower), "<b>x</b>");
    let empty = "<!--StartFragment-->   <!--EndFragment--><p>正文</p>";
    assert_eq!(html_preview_fragment(empty), empty);
}

#[test]
fn html_items_preview_as_html_and_rtf_stays_rich_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    let images = directory.path().join("images");
    let html_id = history.capture_rich(Some("<b>加粗</b>"), None, Some("加粗"), &images)?;
    assert_eq!(
        history.preview_content(html_id)?,
        PreviewContent::Html("<b>加粗</b>".into())
    );
    let rtf_id = history.capture_rich(None, Some(b"{\\rtf1 ascii}"), None, &images)?;
    assert_eq!(
        history.preview_content(rtf_id)?,
        PreviewContent::RichText("[富文本内容]".into())
    );
    Ok(())
}

#[test]
fn html_items_without_stored_html_fall_back_to_rich_text() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let history = History::open(directory.path().join("clipboard.db"))?;
    // 模拟旧数据：直接插入 html 类型但缺 html_content。
    let id = history.repo.insert(crate::database::NewClipboardItem {
        content_type: crate::database::ContentType::Html,
        text_content: Some("旧文本".into()),
        content_hash: "html:legacy".into(),
        semantic_hash: "html:legacy".into(),
        preview: Some("旧文本".into()),
        ..Default::default()
    })?;
    assert_eq!(
        history.preview_content(id)?,
        PreviewContent::RichText("旧文本".into())
    );
    Ok(())
}
