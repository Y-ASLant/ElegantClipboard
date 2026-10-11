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
    let rtf = "{\\rtf1 中文😀{\\*\\datastore{元数据\\}值}}\\rsid123 字\\insrsid456\\mdispDef1 尾}";
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
    let rtf =
        r"{\rtf1 中文😀{\*\datastore{nested \{literal\} \\}}\rsid123\insrsid456\mdispDef1 text}";
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
