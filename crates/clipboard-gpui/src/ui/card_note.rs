use super::DbClipboardItem;
use clipboard_core::search::first_search_match;

pub(super) fn note_title(item: &DbClipboardItem) -> Option<&str> {
    item.note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty())
}

pub(super) fn matching_source<'a>(item: &'a DbClipboardItem, search: &str) -> Option<&'a str> {
    matching_line(note_title(item)?, item.preview.as_deref()?, search)
}

fn matching_line<'a>(note: &str, source: &'a str, search: &str) -> Option<&'a str> {
    if search.trim().is_empty() || first_search_match(note, search).is_some() {
        return None;
    }
    let hit = first_search_match(source, search)?;
    // The single-line clamp must not hide the reason this row matched.
    let breaks = ['\r', '\n', '\u{2028}', '\u{2029}'];
    let before = source[..hit.start]
        .rsplit(breaks)
        .next()
        .unwrap_or_default();
    let start = before
        .char_indices()
        .rev()
        .nth(11)
        .map_or(hit.start - before.len(), |(byte, _)| {
            hit.start - before.len() + byte
        });
    let after = source[hit.end..].split(breaks).next().unwrap_or_default();
    let end = hit.end
        + after
            .char_indices()
            .nth(80)
            .map_or(after.len(), |(byte, _)| byte);
    Some(&source[start..end])
}

#[cfg(test)]
mod tests {
    use super::matching_line;

    #[test]
    fn original_match_is_visible_but_a_matching_note_needs_no_second_line() {
        let original = "first line\n第二行 Execute now\nnot part of the excerpt";
        assert_eq!(
            matching_line("常用命令", original, "execute"),
            Some("第二行 Execute now")
        );
        assert_eq!(matching_line("Execute 服务", original, "execute"), None);
        assert_eq!(matching_line("常用命令", original, ""), None);
        assert_eq!(matching_line("常用命令", original, "absent"), None);
    }
}
