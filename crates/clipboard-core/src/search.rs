use std::ops::Range;

/// Non-overlapping, case-insensitive literal matches with UTF-8 byte offsets.
fn matching_ranges<'a>(text: &'a str, search: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    let mut chars = text.char_indices().peekable();
    let first_needle = search.chars().next().filter(|_| !search.trim().is_empty());
    std::iter::from_fn(move || {
        let first_needle = first_needle?;
        while let Some((start, first)) = chars.next() {
            let mut candidate = chars.clone();
            let mut end = start + first.len_utf8();
            let matched = first.to_lowercase().eq(first_needle.to_lowercase())
                && search.chars().skip(1).all(|expected| {
                    if let Some((byte, actual)) = candidate.next()
                        && actual.to_lowercase().eq(expected.to_lowercase())
                    {
                        end = byte + actual.len_utf8();
                        true
                    } else {
                        false
                    }
                });
            if matched {
                chars = candidate;
                return Some(start..end);
            }
        }
        None
    })
}

/// Search matches in a card preview, with byte ranges safe for styled text.
pub fn search_highlight_ranges(text: &str, search: &str) -> Vec<Range<usize>> {
    matching_ranges(text, search).collect()
}

/// First literal match, without allocating all highlight ranges.
pub fn first_search_match(text: &str, search: &str) -> Option<Range<usize>> {
    matching_ranges(text, search).next()
}

/// True when the saved preview does not already show the term.
pub(crate) fn search_excerpt_needed(preview: &str, search: &str) -> bool {
    !search.trim().is_empty() && first_search_match(preview, search).is_none()
}

/// Show the first matching part of a full text record when its saved preview
/// does not contain the term. Keep the match near the start of a two-line card.
pub fn search_excerpt(full: &str, preview: &str, search: &str) -> Option<String> {
    if first_search_match(preview, search).is_some() {
        return None;
    }
    // Rows can also match via file_paths or note; reject absent terms with a
    // fast byte scan instead of the per-char Unicode comparison below.
    if !full.contains(search) && !full.to_lowercase().contains(&search.to_lowercase()) {
        return None;
    }
    let matched = first_search_match(full, search)?;
    let start = full[..matched.start]
        .char_indices()
        .rev()
        .nth(19)
        .map_or(0, |(byte, _)| byte);
    // Leading line breaks can push the match below a card's line clamp.
    let start = full[start..matched.start]
        .rfind(['\n', '\r', '\u{2028}', '\u{2029}'])
        .map_or(start, |offset| {
            let byte = start + offset;
            byte + full[byte..].chars().next().unwrap().len_utf8()
        });
    let end = full[matched.end..]
        .char_indices()
        .nth(80)
        .map_or(full.len(), |(byte, _)| matched.end + byte);
    let mut excerpt = String::new();
    if start > 0 {
        excerpt.push('…');
    }
    excerpt.push_str(&full[start..end]);
    if end < full.len() {
        excerpt.push('…');
    }
    Some(excerpt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_highlights_keep_utf8_boundaries_and_match_literal_case() {
        let text = "中aBc🙂中文 ABC";
        let ranges = search_highlight_ranges(text, "AbC");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["aBc", "ABC"]
        );
        let ranges = search_highlight_ranges(text, "中");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["中", "中"]
        );
        let ranges = search_highlight_ranges("a.c a-c", "a.c");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0], 0..3);
        assert!(search_highlight_ranges(text, "  ").is_empty());
        assert!(search_highlight_ranges(text, "missing").is_empty());
    }

    #[test]
    fn search_excerpt_exposes_a_match_beyond_the_saved_preview() {
        let full = format!("{}目标{}", "前".repeat(80), "后".repeat(120));
        let excerpt = search_excerpt(&full, "前前前", "目标").unwrap();
        assert!(excerpt.starts_with('…'));
        assert!(excerpt.ends_with('…'));
        assert!(excerpt.contains("目标"));
        assert!(!search_highlight_ranges(&excerpt, "目标").is_empty());
        assert!(search_excerpt(&full, "已显示目标", "目标").is_none());
        assert!(search_excerpt(&full, "前前前", "不存在").is_none());
    }

    #[test]
    fn search_excerpt_keeps_the_match_on_its_first_line() {
        for separator in ["\n\n", "\r\n", "\u{2028}", "\u{2029}"] {
            let full = format!(
                "{}context{separator}目标🙂 trailing\nnext line",
                "前".repeat(80)
            );
            let excerpt = search_excerpt(&full, "前前前", "目标🙂").unwrap();
            assert!(excerpt.lines().next().unwrap().contains("目标🙂"));
            assert_eq!(
                &excerpt[search_highlight_ranges(&excerpt, "目标🙂")[0].clone()],
                "目标🙂"
            );
            assert!(excerpt.ends_with("trailing\nnext line"));
        }
    }

    #[test]
    fn search_preserves_literal_scalar_case_matching() {
        let text = "İ i İ Σσς aaaa";
        assert_eq!(search_highlight_ranges(text, "İ"), vec![0..2, 5..7]);
        assert_eq!(search_highlight_ranges(text, "i"), vec![3..4]);
        let ranges = search_highlight_ranges(text, "σ");
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<Vec<_>>(),
            ["Σ", "σ"]
        );
        assert_eq!(search_highlight_ranges("aaaa", "aa"), vec![0..2, 2..4]);
        assert!(search_excerpt(text, "İ", "İ").is_none());
        assert!(search_excerpt(text, "", "  ").is_none());
    }
}
