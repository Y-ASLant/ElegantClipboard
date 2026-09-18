use crate::{
    HISTORY_LIMIT, History, MAX_TEXT_BYTES,
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
        let total =
            html.map_or(0, str::len) + rtf.map_or(0, <[u8]>::len) + text.map_or(0, str::len);
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
        if let Some(id) = self.repo.touch_by_hash(&hash, None)? {
            return Ok(id);
        }
        let rtf_content = rtf.map(|raw| {
            let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
            format!("{RTF_B64_PREFIX}{}", STANDARD.encode(raw))
        });
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
            preview: Some(text.unwrap_or("[富文本内容]").chars().take(240).collect()),
            byte_size: total as i64,
            char_count: text.map(|value| value.chars().count() as i64),
            ..Default::default()
        })?;
        let (_, deleted_images, deleted_payloads) =
            self.repo.enforce_max_count(HISTORY_LIMIT, None)?;
        self.cleanup_images(deleted_images, images_dir);
        self.cleanup_staged(deleted_payloads, images_dir);
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
            PreviewContent::RichText("中文😀".into())
        );
        assert_eq!(history.list("中文", PAGE_SIZE, false)?[0].id, id);
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
