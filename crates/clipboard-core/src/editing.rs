use crate::{History, MAX_TEXT_BYTES};
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
        if !matches!(item.content_type.as_str(), "text" | "url" | "html" | "rtf") {
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

        let hash = blake3::hash(format!("text:{new_text}").as_bytes())
            .to_hex()
            .to_string();
        let semantic_hash =
            crate::clipboard::semantic_hash_from_text(new_text).unwrap_or_else(|| hash.clone());
        let preview: String = new_text.chars().take(200).collect();
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let affected = tx.execute(
            "UPDATE clipboard_items SET content_type = 'text', text_content = ?1, preview = ?2,
             content_hash = ?3, semantic_hash = ?4, byte_size = ?5, char_count = ?6,
             html_content = NULL, rtf_content = NULL, image_path = NULL,
             image_width = NULL, image_height = NULL, file_paths = NULL, file_payload = NULL
             WHERE id = ?7 AND content_hash = ?8 AND content_type IN ('text', 'url', 'html', 'rtf')",
            params![
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
        tx.commit()?;
        drop(connection);
        if let Some(image_path) = item.image_path {
            self.cleanup_images(vec![image_path], images_dir);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_text_keeps_identity_and_rejects_stale_or_empty_content() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("clipboard.db");
        let history = History::open(path.clone())?;
        let id = history.capture("original")?.unwrap();
        let group = history.create_group("工作")?;
        history.move_to_group(id, None, Some(group.id))?;
        history.toggle_favorite(id)?;
        history.toggle_pin(id)?;
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
}
