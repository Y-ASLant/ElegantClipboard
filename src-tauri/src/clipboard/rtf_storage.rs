//! RTF 原始字节存库/读库（Word 等含 \\binN 二进制段，不能用 lossy UTF-8 字符串）

use base64::Engine;

pub(crate) const RTF_B64_PREFIX: &str = "b64:";

/// 将剪贴板 RTF 原始字节编码为可存入 TEXT 列的字符串
pub(crate) fn encode_rtf_for_storage(raw: &[u8]) -> String {
    let trimmed = if raw.last() == Some(&0) {
        &raw[..raw.len().saturating_sub(1)]
    } else {
        raw
    };
    format!(
        "{RTF_B64_PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(trimmed)
    )
}

fn ensure_rtf_null_terminated(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.last() != Some(&0) {
        bytes.push(0);
    }
    bytes
}

/// 是否为新格式（base64 原始字节）；旧 lossy 字符串不应写回剪贴板
pub(crate) fn should_write_rtf(stored: Option<&str>) -> bool {
    stored
        .filter(|s| !s.is_empty())
        .is_some_and(|s| s.starts_with(RTF_B64_PREFIX))
}

/// 从数据库字段还原为可写入剪贴板的 RTF 字节（仅 b64 格式可信）
pub(crate) fn decode_rtf_for_clipboard(stored: &str) -> Vec<u8> {
    let Some(b64) = stored.strip_prefix(RTF_B64_PREFIX) else {
        return Vec::new();
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return Vec::new();
    };
    ensure_rtf_null_terminated(bytes)
}

/// 按字节上限截断 RTF 存储（保留 b64 编码）
pub(crate) fn truncate_rtf_storage(stored: String, max_size: usize) -> String {
    if max_size == 0 || stored.len() <= max_size {
        return stored;
    }

    if let Some(b64) = stored.strip_prefix(RTF_B64_PREFIX)
        && let Ok(mut bytes) = base64::engine::general_purpose::STANDARD.decode(b64)
    {
        if bytes.len() > max_size {
            bytes.truncate(max_size);
        }
        return encode_rtf_for_storage(&bytes);
    }

    stored.chars().take(max_size).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_binary_rtf() {
        let raw = b"{\\rtf1\\bin2 \\x00\\x01}".to_vec();
        let stored = encode_rtf_for_storage(&raw);
        assert!(stored.starts_with(RTF_B64_PREFIX));
        assert_eq!(
            decode_rtf_for_clipboard(&stored),
            ensure_rtf_null_terminated(raw)
        );
    }

    #[test]
    fn legacy_lossy_rtf_is_not_written_back() {
        let legacy = "{\\rtf1 corrupted \u{FFFD} bytes}".to_string();
        assert!(!should_write_rtf(Some(&legacy)));
        assert!(decode_rtf_for_clipboard(&legacy).is_empty());
    }
}
