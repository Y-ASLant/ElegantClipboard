use crate::{ContentType, History, MAX_TEXT_BYTES};
use anyhow::{Result, bail};
use rusqlite::params;
use std::path::Path;

impl History {
    /// Edit a text-like item only if it still has the content that was previewed.
    /// Rich text and URLs become plain text when their content changes.
    pub fn edit_text(
        &self,
        id: i64,
        expected_hash: &str,
        new_text: &str,
        images_dir: &Path,
    ) -> Result<bool> {
        if new_text.trim().is_empty() {
            bail!("内容不能为空；如需删除，请使用删除按钮");
        }
        if new_text.len() > MAX_TEXT_BYTES {
            bail!("文本超过 1 MiB，未保存");
        }
        let item = self.item(id)?;
        if !item.kind().is_some_and(|kind| kind.is_text_like()) {
            bail!("该类型记录不支持文本编辑");
        }
        if item.file_payload.is_some() {
            bail!("带文件载荷的记录暂不支持编辑");
        }
        if item.content_hash != expected_hash {
            bail!("记录内容已变化，请重新打开后编辑");
        }
        if item.text_content.as_deref() == Some(new_text) {
            return Ok(false);
        }

        let hash = crate::clipboard::hash_with_prefix(b"text:", new_text.as_bytes());
        let semantic_hash =
            crate::clipboard::semantic_hash_from_text(new_text).unwrap_or_else(|| hash.clone());
        let preview: String = crate::truncate_preview(new_text);
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let affected = tx.execute(
            "UPDATE clipboard_items SET content_type = 'text', categories = ?1, text_content = ?2, preview = ?3,
             content_hash = ?4, semantic_hash = ?5, byte_size = ?6, char_count = ?7,
             html_content = NULL, rtf_content = NULL, image_path = NULL,
             image_width = NULL, image_height = NULL, file_paths = NULL, file_payload = NULL
             WHERE id = ?8 AND content_hash = ?9 AND content_type IN ('text', 'url', 'html', 'rtf')",
            params![
                crate::ContentCategory::membership(Some(ContentType::Text), Some(new_text), &[]),
                new_text,
                preview,
                hash,
                semantic_hash,
                i64::try_from(new_text.len())?,
                i64::try_from(new_text.chars().count())?,
                id,
                expected_hash,
            ],
        )?;
        if affected != 1 {
            bail!("记录内容已变化，请重新打开后编辑");
        }
        // 编辑后旧富文本格式失效：丢弃全部额外格式，并把指向旧哈希的别名改指新哈希。
        let blobs = crate::native_format::detach_rows_in_transaction(&tx, &[id])?;
        tx.execute(
            "UPDATE content_hash_aliases SET canonical_hash = ?1 WHERE canonical_hash = ?2",
            params![hash, expected_hash],
        )?;
        tx.commit()?;
        drop(connection);
        let images = item.image_path.into_iter().collect();
        let _ = self.cleanup_media(images, Vec::new(), blobs, images_dir);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_types_remain_previewable_but_cannot_be_edited() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let history = History::open(directory.path().join("clipboard.db"))?;
        let id = history.capture("legacy text")?.unwrap();
        let original = history.item(id)?;
        for kind in ["future-format", "", "Text"] {
            history.db.write_connection().lock().execute(
                "UPDATE clipboard_items SET content_type = ?1 WHERE id = ?2",
                params![kind, id],
            )?;
            assert!(matches!(
                history.preview_content(id)?,
                crate::PreviewContent::Text(text) if text == "legacy text"
            ));
            for replacement in ["legacy text", "changed"] {
                assert!(
                    history
                        .edit_text(id, &original.content_hash, replacement, directory.path())
                        .is_err()
                );
            }
            let unchanged = history.item(id)?;
            assert_eq!(unchanged.content_type, kind);
            assert_eq!(unchanged.content_hash, original.content_hash);
            assert_eq!(unchanged.text_content, original.text_content);
        }
        Ok(())
    }

    #[test]
    fn edit_text_keeps_identity_and_rejects_stale_or_empty_content() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let id = history.capture("original")?.unwrap();
        let group = history.create_group("工作")?;
        history.move_to_group(id, None, Some(group.id))?;
        history.toggle_favorite(id)?;
        history.toggle_pin(id, false)?;
        let hash = history.item(id)?.content_hash;
        assert!(!history.edit_text(id, &hash, "original", dir.path())?);
        assert!(history.edit_text(id, &hash, " ", dir.path()).is_err());
        assert!(
            history
                .edit_text(id, &hash, &"x".repeat(MAX_TEXT_BYTES + 1), dir.path())
                .is_err()
        );
        assert!(
            history
                .edit_text(id, "stale", "changed", dir.path())
                .is_err()
        );
        assert!(history.edit_text(id, &hash, "中文 edited", dir.path())?);
        assert!(history.edit_text(id, &hash, "second", dir.path()).is_err());
        let item = history.item(id)?;
        assert_eq!(item.group_id, Some(group.id));
        assert!(item.is_pinned && item.is_favorite);
        assert_eq!(item.preview.as_deref(), Some("中文 edited"));
        assert_eq!(history.count_in_group("edited", false, Some(group.id))?, 1);
        drop(history);
        assert_eq!(History::open(path)?.text(id)?, "中文 edited");
        Ok(())
    }

    #[test]
    fn edited_url_stays_text_after_database_reopen() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let id = history.capture("https://example.com/old")?.unwrap();
        let item = history.item(id)?;
        assert_eq!(item.content_type, "url");
        assert!(history.edit_text(
            id,
            &item.content_hash,
            "https://example.com/edited",
            dir.path(),
        )?);
        assert_eq!(history.item(id)?.content_type, "text");
        drop(history);

        let reopened = History::open(path)?;
        assert_eq!(reopened.item(id)?.content_type, "text");
        assert_eq!(reopened.text(id)?, "https://example.com/edited");
        Ok(())
    }

    #[test]
    fn rich_text_edit_downgrades_to_plain_text_only_when_changed() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let images = dir.path().join("images");
        let id = history.capture_rich(Some("<b>hello</b>"), None, Some("hello"), &images)?;
        let hash = history.item(id)?.content_hash;
        assert!(!history.edit_text(id, &hash, "hello", &images)?);
        assert_eq!(history.item(id)?.content_type, "html");
        assert!(history.edit_text(id, &hash, "hello again", &images)?);
        let item = history.item(id)?;
        assert_eq!(item.content_type, "text");
        assert!(item.html_content.is_none());
        assert!(item.rtf_content.is_none());
        assert_eq!(history.text(id)?, "hello again");
        Ok(())
    }

    #[test]
    fn edit_releases_unreferenced_native_blobs_and_keeps_shared() -> Result<()> {
        use crate::native_format::{NATIVE_INLINE_MAX_BYTES, NativeFormatCapture};

        let dir = tempfile::tempdir()?;
        let history = History::open(dir.path().join("clipboard.db"))?;
        let images = dir.path().join("images");
        let payload = vec![9u8; NATIVE_INLINE_MAX_BYTES + 1];
        let alone = history.capture_rich(Some("<b>alone</b>"), None, Some("alone"), &images)?;
        history.attach_native_formats(
            alone,
            vec![NativeFormatCapture {
                name: "Example.Custom".into(),
                data: payload.clone(),
            }],
        )?;
        let shared_a = history.capture_rich(Some("<b>a</b>"), None, Some("a"), &images)?;
        let shared_b = history.capture_rich(Some("<b>b</b>"), None, Some("b"), &images)?;
        let shared_payload = vec![3u8; NATIVE_INLINE_MAX_BYTES + 2];
        history.attach_native_formats(
            shared_a,
            vec![NativeFormatCapture {
                name: "Shared.Fmt".into(),
                data: shared_payload.clone(),
            }],
        )?;
        history.attach_native_formats(
            shared_b,
            vec![NativeFormatCapture {
                name: "Shared.Fmt".into(),
                data: shared_payload,
            }],
        )?;
        let blobs = dir.path().join("blobs");
        assert_eq!(std::fs::read_dir(&blobs)?.count(), 2);

        let hash = history.item(alone)?.content_hash;
        assert!(history.edit_text(alone, &hash, "edited alone", &images)?);
        assert!(history.native_formats_for_copy(alone)?.is_empty());
        assert_eq!(std::fs::read_dir(&blobs)?.count(), 1, "独有 blob 应回收");

        let hash_a = history.item(shared_a)?.content_hash;
        assert!(history.edit_text(shared_a, &hash_a, "edited a", &images)?);
        assert_eq!(
            std::fs::read_dir(&blobs)?.count(),
            1,
            "共享 blob 在仍有引用时保留"
        );
        assert_eq!(
            history.native_formats_for_copy(shared_b)?.len(),
            1,
            "另一条共享引用仍可读"
        );
        Ok(())
    }
}
