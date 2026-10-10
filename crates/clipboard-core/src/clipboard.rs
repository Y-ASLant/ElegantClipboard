use blake3::Hasher;

const ZERO_WIDTH_CHARS: [char; 5] = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'];

pub(crate) fn hash_with_prefix(prefix: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(prefix);
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

/// Normalize user-visible text so semantically equivalent clipboard text
/// (line endings, zero-width chars, trailing spaces/tabs) hashes consistently.
pub(crate) fn normalize_semantic_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ZERO_WIDTH_CHARS.contains(&ch) {
            continue;
        }
        if ch == '\n' || ch == '\r' {
            while normalized.ends_with([' ', '\t']) {
                normalized.pop();
            }
            normalized.push('\n');
            if ch == '\r' {
                chars.next_if_eq(&'\n');
            }
        } else if ch == '\u{00A0}' {
            normalized.push(' ');
        } else {
            normalized.push(ch);
        }
    }
    while normalized.ends_with([' ', '\t', '\n']) {
        normalized.pop();
    }
    normalized
}

pub(crate) fn semantic_hash_from_text(text: &str) -> Option<String> {
    let normalized = normalize_semantic_text(text);
    if normalized.is_empty() {
        return None;
    }
    Some(hash_with_prefix(b"text:", normalized.as_bytes()))
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// 从 URL 的 authority 部分提取 host（去掉端口、路径、查询串）。
fn extract_host(authority: &str) -> &str {
    let authority = authority.split(['/', '?', '#']).next().unwrap_or(authority);
    if authority.starts_with('[') {
        if let Some(end) = authority.find(']') {
            return &authority[..=end];
        }
        return authority;
    }
    authority.split(':').next().unwrap_or(authority)
}

fn is_valid_ipv4(host: &str) -> bool {
    let mut parts = host.split('.');
    for _ in 0..4 {
        if !parts.next().is_some_and(|part| part.parse::<u8>().is_ok()) {
            return false;
        }
    }
    parts.next().is_none()
}

fn is_valid_ipv6(host: &str) -> bool {
    if !host.starts_with('[') || !host.ends_with(']') || host.len() <= 2 {
        return false;
    }
    let inner = &host[1..host.len() - 1];
    !inner.is_empty()
        && inner
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.')
}

fn is_valid_domain_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || !host.is_ascii() {
        return false;
    }

    let Some((_, tld)) = host.rsplit_once('.') else {
        return false;
    };

    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
    }

    // TLD 至少 2 字符且包含至少一个字母（兼容 punycode 如 xn--g6q252g）
    tld.len() >= 2 && tld.bytes().any(|b| b.is_ascii_alphabetic())
}

fn is_valid_url_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if is_valid_ipv4(host) || is_valid_ipv6(host) {
        return true;
    }
    is_valid_domain_host(host)
}

fn is_url_remainder(remainder: &str) -> bool {
    let host = extract_host(remainder);
    is_valid_url_host(host)
}

/// 若为 URL 则返回 trim 后的规范文本（用于哈希与入库保持一致）。
pub(crate) fn canonical_url_text(text: &str) -> Option<&str> {
    is_url(text).then(|| text.trim())
}

/// 判断纯文本是否为单行 URL（用于归类到「其它」）
pub(crate) fn is_url(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.lines().count() != 1 {
        return false;
    }
    if trimmed.chars().any(char::is_whitespace) {
        return false;
    }

    if starts_with_ignore_ascii_case(trimmed, "http://") {
        return trimmed.get(7..).is_some_and(is_url_remainder);
    }
    if starts_with_ignore_ascii_case(trimmed, "https://") {
        return trimmed.get(8..).is_some_and(is_url_remainder);
    }
    if starts_with_ignore_ascii_case(trimmed, "ftp://") {
        return trimmed.get(6..).is_some_and(is_url_remainder);
    }
    if starts_with_ignore_ascii_case(trimmed, "www.") {
        return trimmed.get(4..).is_some_and(is_url_remainder);
    }

    false
}

/// Strip volatile fields from RTF that Word/Outlook regenerate on every copy,
/// so identical user-visible content produces the same hash.
///
/// Removed (matching Ditto's `GenerateCRC` logic):
/// - `{\*\datastore{...}}` block (Word's rich-data XML, changes per document instance)
/// - `\rsid`, `\insrsid` control words with trailing decimal digits
/// - `\mdispDef1` control word (no numeric arg)
pub(crate) fn normalize_rtf_for_hash(rtf_bytes: &[u8]) -> Vec<u8> {
    if std::str::from_utf8(rtf_bytes).is_err() {
        return rtf_bytes.to_vec();
    }

    let mut out = Vec::with_capacity(rtf_bytes.len());
    let len = rtf_bytes.len();
    let mut i = 0;

    while i < len {
        // Escaped backslashes and braces are literal text, not RTF syntax.
        if rtf_bytes[i] == b'\\' && matches!(rtf_bytes.get(i + 1), Some(b'\\' | b'{' | b'}')) {
            out.extend_from_slice(&rtf_bytes[i..i + 2]);
            i += 2;
            continue;
        }

        let remaining = &rtf_bytes[i..];
        // Only skip a complete datastore group.
        if remaining.starts_with(br"{\*\datastore")
            && !remaining
                .get(br"{\*\datastore".len())
                .is_some_and(u8::is_ascii_alphabetic)
        {
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < len {
                match rtf_bytes[j] {
                    b'\\' => j += 1, // Skip escaped braces and backslashes.
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 {
                // Keep the historical opening-brace placeholder for stored hash compatibility.
                out.push(b'{');
                i = j + 1;
                continue;
            }
            // Preserve an unterminated group without repeatedly rescanning it.
            out.extend_from_slice(remaining);
            break;
        }

        // Match \rsidXXXXX / \insrsidXXXXX — strip control word + trailing digits.
        // Only strip when digits actually follow, so \rsidtbl, \rsidroot etc. are untouched.
        let cmd_len = if remaining.starts_with(br"\insrsid") {
            Some(br"\insrsid".len())
        } else if remaining.starts_with(br"\rsid") {
            Some(br"\rsid".len())
        } else if remaining.starts_with(br"\mdispDef1") {
            // \mdispDef1 has no numeric arg — always strip
            let j = i + br"\mdispDef1".len();
            i = j + if j < len && rtf_bytes[j] == b' ' {
                1
            } else {
                0
            };
            continue;
        } else {
            None
        };

        if let Some(cmd_len) = cmd_len {
            let after_cmd = i + cmd_len;
            // Skip optional space between control word and digits
            let digit_start = if after_cmd < len && rtf_bytes[after_cmd] == b' ' {
                after_cmd + 1
            } else {
                after_cmd
            };
            // Scan trailing digits (Word RSID is decimal, not hex)
            let mut j = digit_start;
            while j < len && rtf_bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > digit_start {
                // Found digits — strip the whole thing
                i = j;
                continue;
            }
            // No digits found — don't strip (e.g. \rsidtbl, \rsidroot)
        }

        out.push(rtf_bytes[i]);
        i += 1;
    }

    out
}

pub(crate) fn compute_semantic_hash(
    content_type: &str,
    text_content: Option<&str>,
    content_hash: &str,
) -> String {
    let is_text_like = content_type.eq_ignore_ascii_case("text")
        || content_type.eq_ignore_ascii_case("html")
        || content_type.eq_ignore_ascii_case("rtf");
    if is_text_like
        && let Some(text) = text_content
        && let Some(hash) = semantic_hash_from_text(text)
    {
        return hash;
    }
    content_hash.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_url_text, compute_semantic_hash, is_url, normalize_rtf_for_hash,
        normalize_semantic_text, semantic_hash_from_text,
    };

    #[test]
    fn normalize_text_removes_invisible_chars_and_trailing_whitespace() {
        let input = "A\u{200B}\u{00A0}B\t  \r\nline 2\t\n\n";
        let normalized = normalize_semantic_text(input);
        assert_eq!(normalized, "A B\nline 2");
    }

    #[test]
    fn compute_semantic_hash_accepts_uppercase_content_type() {
        let text_hash = compute_semantic_hash("TEXT", Some("hello"), "fallback");
        let html_hash = compute_semantic_hash("HTML", Some("hello"), "fallback");
        let rtf_hash = compute_semantic_hash("RTF", Some("hello"), "fallback");

        assert_eq!(text_hash, html_hash);
        assert_eq!(text_hash, rtf_hash);
        assert_ne!(text_hash, "fallback");
    }

    #[test]
    fn normalize_empty_string() {
        assert_eq!(normalize_semantic_text(""), "");
    }

    #[test]
    fn normalize_whitespace_only() {
        assert_eq!(normalize_semantic_text("  \t\n\r\n  "), "");
    }

    #[test]
    fn normalize_preserves_cjk() {
        assert_eq!(normalize_semantic_text("你好世界"), "你好世界");
    }

    #[test]
    fn normalize_preserves_emoji() {
        assert_eq!(normalize_semantic_text("hello 🎉 world"), "hello 🎉 world");
    }

    #[test]
    fn normalize_mixed_line_endings() {
        assert_eq!(normalize_semantic_text("a\r\nb\rc\n"), "a\nb\nc");
    }

    #[test]
    fn normalize_only_collapses_adjacent_crlf() {
        assert_eq!(
            normalize_semantic_text("a \r\u{200B}\nb\u{00A0}\n \tc"),
            "a\n\nb\n \tc"
        );
    }

    #[test]
    fn semantic_hash_empty_returns_none() {
        assert_eq!(semantic_hash_from_text(""), None);
        assert_eq!(semantic_hash_from_text("  \t"), None);
    }

    #[test]
    fn semantic_hash_same_text_same_hash() {
        let h1 = semantic_hash_from_text("hello world");
        let h2 = semantic_hash_from_text("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn semantic_hash_different_text_different_hash() {
        let h1 = semantic_hash_from_text("hello");
        let h2 = semantic_hash_from_text("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn compute_semantic_hash_non_text_uses_fallback() {
        let hash = compute_semantic_hash("image", Some("hello"), "fallback_hash");
        assert_eq!(hash, "fallback_hash");
    }

    #[test]
    fn compute_semantic_hash_text_without_content_uses_fallback() {
        let hash = compute_semantic_hash("text", None, "fallback_hash");
        assert_eq!(hash, "fallback_hash");
    }

    #[test]
    fn is_url_accepts_http_and_https() {
        assert!(is_url("https://example.com/path?q=1"));
        assert!(is_url("http://example.com"));
        assert!(is_url("  https://a.co  "));
        assert!(is_url("http://127.0.0.1/path"));
        assert!(is_url("https://localhost:8080"));
        assert!(is_url("https://xn--fiqs8s.xn--fiqs8s.xn--g6q252g"));
    }

    #[test]
    fn is_url_accepts_www_prefix() {
        assert!(is_url("www.example.com/page"));
    }

    #[test]
    fn is_url_rejects_multiline_and_plain_text() {
        assert!(!is_url("hello world"));
        assert!(!is_url("https://a.com\nline2"));
        assert!(!is_url("visit https://example.com"));
        assert!(!is_url("http://"));
        assert!(!is_url("www."));
        assert!(!is_url("http://example"));
        assert!(!is_url("https://a"));
    }

    #[test]
    fn is_url_rejects_loose_false_positives() {
        assert!(!is_url("www.这不是网址.测试"));
        assert!(!is_url("www.a.b"));
        assert!(!is_url("http://x.y"));
    }

    #[test]
    fn is_url_does_not_panic_on_cjk_text() {
        assert!(!is_url("不等于程序空闲时自己崩"));
        assert!(!is_url("我们"));
        assert!(!is_url("你好世界"));
    }

    #[test]
    fn canonical_url_text_trims_whitespace() {
        assert_eq!(
            canonical_url_text("  https://example.com  "),
            Some("https://example.com")
        );
        assert_eq!(canonical_url_text("hello"), None);
    }

    #[test]
    fn rtf_strips_datastore_block_preserving_historical_hash_bytes() {
        // Standard RTF form: {\*\datastore {...}}
        let rtf = br"{\rtf1\ansi hello {\*\datastore {some xml {nested}}}\par}";
        assert_eq!(normalize_rtf_for_hash(rtf), br"{\rtf1\ansi hello {\par}");
    }

    #[test]
    fn rtf_preserves_malformed_datastore_groups() {
        for rtf in [
            "",
            "\\",
            r"\*\datastore",
            r"\*\datastore}",
            r"\*\datastore{payload}",
            r"{visible \*\datastore{payload}}",
            r"}\*\datastore",
            r"{\*\datastore",
            r"{\*\datastore{payload}",
            r"{\*\datastore{\*\datastore{payload}",
            r"{\*\datastore payload\}",
            r"{\*\datastore payload\",
            r"{\*\datastoreExtra{visible}}",
            r"\{\*\datastore{visible}}",
        ] {
            assert_eq!(normalize_rtf_for_hash(rtf.as_bytes()), rtf.as_bytes());
        }
    }

    #[test]
    fn rtf_datastore_respects_escaped_braces_and_backslashes() {
        for rtf in [
            r"before{\*\datastore left\{nested{payload}}after",
            r"before{\*\datastore right\}nested{payload}}after",
            r"before{\*\datastore slash\\}after",
            r"before{\*\datastore slash\\{nested}}after",
            r"before{\*\datastore slash\\\}nested{payload}}after",
        ] {
            assert_eq!(normalize_rtf_for_hash(rtf.as_bytes()), b"before{after");
        }
        assert_eq!(
            normalize_rtf_for_hash(br"before{\*\datastore{one}}{\*\datastore{two}}after"),
            b"before{{after"
        );
        assert_eq!(
            normalize_rtf_for_hash(br"\\{\*\datastore{payload}}after"),
            br"\\{after"
        );
        assert_eq!(normalize_rtf_for_hash(br"{\*\datastore}"), b"{");
    }

    #[test]
    fn rtf_preserves_escaped_control_words() {
        let rtf = br"{\rtf1 \\rsid123 \\insrsid456 \\mdispDef1 \{\*\datastore\}}";
        assert_eq!(normalize_rtf_for_hash(rtf), rtf);
    }

    #[test]
    fn rtf_normalization_preserves_utf8_text() {
        let rtf =
            "{\\rtf1 中文😀{\\*\\datastore{元数据\\}值}}\\rsid123 字\\insrsid456\\mdispDef1 尾}";
        assert_eq!(
            normalize_rtf_for_hash(rtf.as_bytes()),
            "{\\rtf1 中文😀{ 字尾}".as_bytes()
        );
        for rtf in ["中文😀", r"中文\*\datastore", r"{\*\datastore{未闭合😀}"] {
            assert_eq!(normalize_rtf_for_hash(rtf.as_bytes()), rtf.as_bytes());
        }
    }

    #[test]
    fn rtf_truncated_controls_and_groups_terminate() {
        let rtf = r"{\rtf1 中文😀{\*\datastore{nested \{literal\} \\}}\rsid123\insrsid456\mdispDef1 text}";
        for end in 0..=rtf.len() {
            let prefix = &rtf.as_bytes()[..end];
            let normalized = normalize_rtf_for_hash(prefix);
            assert!(normalized.len() <= prefix.len());
            if std::str::from_utf8(prefix).is_err() {
                assert_eq!(normalized, prefix);
            } else {
                assert!(std::str::from_utf8(&normalized).is_ok());
            }
        }
    }

    #[test]
    fn rtf_strips_rsid_and_inrsid() {
        let rtf = br"{\rtf1\ansi hello\rsid12345 \insrsid67890 world}";
        let cleaned = normalize_rtf_for_hash(rtf);
        let s = String::from_utf8(cleaned).unwrap();
        assert!(!s.contains("rsid12345"));
        assert!(!s.contains("insrsid67890"));
        assert!(s.contains("hello"));
        assert!(s.contains("world"));
    }

    #[test]
    fn rtf_preserves_rsid_without_digits() {
        // \rsidroot, \rsidtbl etc. have no trailing digits — must be preserved
        let rtf = br"{\rtf1\ansi {\rsidtbl {\rsid12345}}\rsidroot hello}";
        let cleaned = normalize_rtf_for_hash(rtf);
        let s = String::from_utf8(cleaned).unwrap();
        assert!(s.contains("rsidtbl"), "rsidtbl should be preserved");
        assert!(s.contains("rsidroot"), "rsidroot should be preserved");
        assert!(!s.contains("rsid12345"), "rsid12345 should be stripped");
    }

    #[test]
    fn rtf_strips_mdispdef1() {
        let rtf = br"{\rtf1\ansi hello\mdispDef1 world}";
        let cleaned = normalize_rtf_for_hash(rtf);
        let s = String::from_utf8(cleaned).unwrap();
        assert!(!s.contains("mdispDef1"));
        assert!(s.contains("hello"));
        assert!(s.contains("world"));
    }

    #[test]
    fn rtf_preserves_rsidtbl() {
        let rtf = br"{\rtf1\ansi {\rsidtbl \rsid12345}}";
        let cleaned = normalize_rtf_for_hash(rtf);
        let s = String::from_utf8(cleaned).unwrap();
        // \rsidtbl should be preserved; only standalone \rsid is stripped
        assert!(s.contains("rsidtbl"));
    }

    #[test]
    fn rtf_same_content_different_rsid_same_hash() {
        let rtf_a = br"{\rtf1\ansi hello\rsid11111 world}";
        let rtf_b = br"{\rtf1\ansi hello\rsid99999 world}";
        let a = normalize_rtf_for_hash(rtf_a);
        let b = normalize_rtf_for_hash(rtf_b);
        assert_eq!(a, b);
    }

    #[test]
    fn rtf_different_content_still_differs() {
        let rtf_a = br"{\rtf1\ansi hello}";
        let rtf_b = br"{\rtf1\ansi world}";
        let a = normalize_rtf_for_hash(rtf_a);
        let b = normalize_rtf_for_hash(rtf_b);
        assert_ne!(a, b);
    }

    #[test]
    fn rtf_invalid_utf8_returns_original() {
        let rtf: &[u8] = &[0xFF, 0xFE, 0x00];
        let cleaned = normalize_rtf_for_hash(rtf);
        assert_eq!(cleaned, rtf);
    }
}
