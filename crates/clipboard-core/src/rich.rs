use crate::{
    History, MAX_TEXT_BYTES,
    database::{ContentType, NewClipboardItem},
};
use anyhow::{Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::path::Path;

const RTF_B64_PREFIX: &str = "b64:";

pub fn decode_rtf_for_clipboard(stored: &str) -> Option<Vec<u8>> {
    let encoded = stored.strip_prefix(RTF_B64_PREFIX)?;
    if encoded.len() > MAX_TEXT_BYTES * 2 {
        return None;
    }
    let mut bytes = STANDARD.decode(encoded).ok()?;
    if bytes.is_empty() {
        return None;
    }
    if bytes.last() != Some(&0) {
        bytes.push(0);
    }
    Some(bytes)
}

/// 存储编码：去掉 NUL 终止符后 base64（同 capture_rich 的 rtf_content 规则）。
pub(crate) fn encode_rtf_for_storage(raw: &[u8]) -> String {
    let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
    format!("{RTF_B64_PREFIX}{}", STANDARD.encode(raw))
}

impl History {
    pub fn capture_rich(
        &self,
        html: Option<&str>,
        rtf: Option<&[u8]>,
        text: Option<&str>,
        images_dir: &Path,
    ) -> Result<i64> {
        let html = html.filter(|value| !value.is_empty());
        let rtf = rtf.filter(|value| !value.is_empty());
        if html.is_none() && rtf.is_none() {
            bail!("没有可保存的富文本内容");
        }
        let text = text.filter(|value| !value.is_empty());
        let mut total =
            html.map_or(0, str::len) + rtf.map_or(0, <[u8]>::len) + text.map_or(0, str::len);
        if total > MAX_TEXT_BYTES {
            bail!("富文本内容超过 1 MiB，未保存");
        }
        let fallback = if text.is_none() {
            html.map(crate::html_text::html_to_text).transpose()?
        } else {
            None
        };
        let text = text.or_else(|| fallback.as_deref().filter(|value| !value.is_empty()));
        total += fallback.as_ref().map_or(0, String::len);
        if total > MAX_TEXT_BYTES {
            bail!("富文本内容超过 1 MiB，未保存");
        }
        let mut hasher = blake3::Hasher::new();
        if let Some(html) = html {
            hasher.update(b"html:");
            hasher.update(html.as_bytes());
        } else if let Some(rtf) = rtf {
            hasher.update(b"rtf:");
            let raw = rtf.strip_suffix(&[0]).unwrap_or(rtf);
            if raw.is_ascii() {
                hasher.update(&crate::clipboard::normalize_rtf_for_hash(raw));
            } else {
                hasher.update(raw);
            }
        }
        let hash = hasher.finalize().to_hex().to_string();
        if self.dedup_allows_reuse() {
            if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
                return Ok(id);
            }
            // 同文合并：默认分组内已有相同纯文本的条目时并入而不新建。
            if let Some(text) = text
                && let Some(target) = self.find_same_text_item(text)?
            {
                let input = crate::same_text::MergeInput {
                    content_type: if html.is_some() {
                        ContentType::Html
                    } else {
                        ContentType::Rtf
                    },
                    html,
                    rtf,
                    text,
                    hash: hash.clone(),
                };
                return self.merge_same_text(&target, &input, Some(images_dir));
            }
        }
        let rtf_content = rtf.map(encode_rtf_for_storage);
        let id = self.repo.insert(NewClipboardItem {
            content_type: if html.is_some() {
                ContentType::Html
            } else {
                ContentType::Rtf
            },
            text_content: text.map(str::to_owned),
            html_content: html.map(str::to_owned),
            rtf_content,
            content_hash: hash.clone(),
            semantic_hash: text
                .and_then(crate::clipboard::semantic_hash_from_text)
                .unwrap_or(hash),
            preview: Some(crate::truncate_preview(text.unwrap_or("[富文本内容]"))),
            byte_size: total as i64,
            char_count: text.map(|value| value.chars().count() as i64),
            ..Default::default()
        })?;
        self.enforce_history_limit(Some(images_dir))?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PAGE_SIZE, PreviewContent};

    #[test]
    fn rich_history_preserves_html_and_binary_rtf() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let html = "<b>中文😀</b>";
        let rtf = b"{\\rtf1\\bin2 \x00\x01}\0";
        let id = history.capture_rich(Some(html), Some(rtf), Some("中文😀"), &images)?;
        assert_eq!(
            history.capture_rich(Some(html), Some(rtf), Some("中文😀"), &images)?,
            id
        );
        assert_eq!(history.count("", false)?, 1);
        let item = history.item(id)?;
        assert_eq!(item.content_type, "html");
        assert_eq!(
            item.content_hash,
            blake3::hash(format!("html:{html}").as_bytes())
                .to_hex()
                .to_string()
        );
        assert_eq!(item.html_content.as_deref(), Some(html));
        assert_eq!(
            decode_rtf_for_clipboard(item.rtf_content.as_deref().unwrap()),
            Some(rtf.to_vec())
        );
        assert_eq!(
            history.preview_content(id)?,
            PreviewContent::Html("<b>中文😀</b>".into())
        );
        assert_eq!(history.list("中文", PAGE_SIZE, false)?[0].id, id);
        Ok(())
    }

    #[test]
    fn html_fallback_is_searchable_and_source_text_including_whitespace_wins() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let html = "<p>First &amp; second</p><p>末行</p>";
        let id = history.capture_rich(Some(html), None, None, &images)?;
        let item = history.item(id)?;
        let expected = "First & second\n末行";
        assert_eq!(item.text_content.as_deref(), Some(expected));
        assert_eq!(item.preview.as_deref(), Some(expected));
        assert_eq!(item.char_count, Some(expected.chars().count() as i64));
        assert_eq!(item.byte_size, (html.len() + expected.len()) as i64);
        assert_eq!(history.list("末行", PAGE_SIZE, false)?[0].id, id);
        for (html, source) in [("<p>different</p>", " source\n"), ("<p>ignored</p>", " \t")] {
            let id = history.capture_rich(Some(html), None, Some(source), &images)?;
            assert_eq!(history.item(id)?.text_content.as_deref(), Some(source));
        }
        let id = history.capture_rich(Some("<p>empty source</p>"), None, Some(""), &images)?;
        assert_eq!(
            history.item(id)?.text_content.as_deref(),
            Some("empty source")
        );
        Ok(())
    }

    #[test]
    fn rtf_only_and_legacy_lossy_data_are_distinguished() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let id = history.capture_rich(
            None,
            Some(b"{\\rtf1 test}"),
            None,
            &directory.path().join("images"),
        )?;
        assert_eq!(history.item(id)?.content_type, "rtf");
        assert_eq!(
            history.preview_content(id)?,
            PreviewContent::RichText("[富文本内容]".into())
        );
        assert!(decode_rtf_for_clipboard("{\\rtf1 lossy �}").is_none());
        assert!(decode_rtf_for_clipboard("b64:invalid!").is_none());
        Ok(())
    }

    #[test]
    fn rtf_hash_ignores_volatile_word_ids() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let first =
            history.capture_rich(None, Some(b"{\\rtf1\\rsid123 text}"), Some("text"), &images)?;
        let second =
            history.capture_rich(None, Some(b"{\\rtf1\\rsid456 text}"), Some("text"), &images)?;
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn rtf_hash_ignores_word_datastore_and_preserves_original_payload() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let first_rtf =
            br"{\rtf1{\*\datastore{first \} metadata}}\rsid123\insrsid456\mdispDef1 text}";
        let second_rtf = br"{\rtf1{\*\datastore{second {nested}}}\rsid789\insrsid012text}";
        let first = history.capture_rich(None, Some(first_rtf), None, &images)?;
        let second = history.capture_rich(None, Some(second_rtf), None, &images)?;
        assert_eq!(first, second);
        assert_eq!(history.count("", false)?, 1);
        let item = history.item(first)?;
        assert_eq!(
            item.content_hash,
            crate::clipboard::hash_with_prefix(b"rtf:", br"{\rtf1{text}")
        );
        let mut original = first_rtf.to_vec();
        original.push(0);
        assert_eq!(
            decode_rtf_for_clipboard(item.rtf_content.as_deref().unwrap()),
            Some(original)
        );
        assert_ne!(
            history.capture_rich(None, Some(br"{\rtf1different}"), None, &images)?,
            first
        );
        Ok(())
    }

    #[test]
    fn rtf_capture_preserves_malformed_datastore_and_non_ascii_payloads() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        let inputs: &[&[u8]] = &[
            br"\*\datastore",
            br"\*\datastore}",
            br"{\*\datastore{payload}",
            br"{\*\datastore payload\}",
            br"\{\*\datastore\}",
            "{\\rtf1 中文😀\\rsid123}".as_bytes(),
            b"{\\rtf1 \\rsid123 \xff}",
        ];
        for &rtf in inputs {
            let id = history.capture_rich(None, Some(rtf), None, &images)?;
            let item = history.item(id)?;
            assert_eq!(
                item.content_hash,
                crate::clipboard::hash_with_prefix(b"rtf:", rtf)
            );
            let mut terminated = rtf.to_vec();
            terminated.push(0);
            assert_eq!(
                history.capture_rich(None, Some(&terminated), None, &images)?,
                id
            );
            assert_eq!(
                decode_rtf_for_clipboard(item.rtf_content.as_deref().unwrap()),
                Some(terminated)
            );
        }
        assert_eq!(history.count("", false)?, inputs.len() as i64);
        Ok(())
    }

    #[test]
    fn oversized_rich_content_is_rejected() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let images = directory.path().join("images");
        assert!(
            history
                .capture_rich(None, None, Some("plain"), &images)
                .is_err()
        );
        assert!(
            history
                .capture_rich(Some(&"x".repeat(MAX_TEXT_BYTES + 1)), None, None, &images)
                .is_err()
        );
        assert_eq!(history.count("", false)?, 0);
        Ok(())
    }

    #[test]
    fn rich_items_reorder_with_plain_text() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let rich = history.capture_rich(
            Some("<b>rich</b>"),
            None,
            Some("rich"),
            &directory.path().join("images"),
        )?;
        let plain = history.capture("plain")?.unwrap();
        history.reorder(rich, plain, false, false)?;
        assert_eq!(history.list("", PAGE_SIZE, false)?[0].id, rich);
        drop(history);
        assert_eq!(History::open(path)?.list("", PAGE_SIZE, false)?[0].id, rich);
        Ok(())
    }
}
