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
mod tests;
