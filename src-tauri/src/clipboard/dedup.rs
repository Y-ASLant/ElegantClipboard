use blake3::Hasher;

const ZERO_WIDTH_CHARS: [char; 5] = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}'];

fn hash_with_prefix(prefix: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(prefix);
    hasher.update(bytes);
    hasher.finalize().to_hex().to_string()
}

/// Normalize user-visible text so semantically equivalent clipboard text
/// (line endings, zero-width chars, trailing spaces) hashes consistently.
pub(crate) fn normalize_semantic_text(text: &str) -> String {
    let with_lf = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cleaned = String::with_capacity(with_lf.len());
    for ch in with_lf.chars() {
        if ZERO_WIDTH_CHARS.contains(&ch) {
            continue;
        }
        if ch == '\u{00A0}' {
            cleaned.push(' ');
        } else {
            cleaned.push(ch);
        }
    }

    let mut normalized = String::with_capacity(cleaned.len());
    for (i, line) in cleaned.split('\n').enumerate() {
        if i > 0 {
            normalized.push('\n');
        }
        normalized.push_str(line.trim_end_matches([' ', '\t']));
    }

    while normalized.ends_with('\n') {
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

pub(crate) fn compute_semantic_hash(
    content_type: &str,
    text_content: Option<&str>,
    content_hash: &str,
) -> String {
    let is_text_like = matches!(content_type, "text" | "html" | "rtf");
    if is_text_like
        && let Some(text) = text_content
        && let Some(hash) = semantic_hash_from_text(text)
    {
        return hash;
    }
    content_hash.to_string()
}
