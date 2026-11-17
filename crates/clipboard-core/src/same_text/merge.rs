use crate::History;
use crate::same_text::MergeInput;
use anyhow::Result;
use rusqlite::OptionalExtension;

/// 同文合并目标的窄列投影：合并只需这四个字段，避免载入富文本大列。
pub(crate) struct SameTextTarget {
    pub id: i64,
    pub content_type: String,
    pub content_hash: String,
    pub preview: Option<String>,
}

impl History {
    pub(crate) fn find_same_text_item(&self, text: &str) -> Result<Option<SameTextTarget>> {
        let connection = self.db.read_connection();
        let connection = connection.lock();
        let mut statement = connection.prepare(
            "SELECT id, content_type, content_hash, preview FROM clipboard_items
             WHERE text_content = ?1 AND group_id IS NULL
               AND content_type IN ('text','url','html','rtf')
             ORDER BY sort_order DESC, created_at DESC, id DESC LIMIT 1",
        )?;
        let target = statement
            .query_row(rusqlite::params![text], |row| {
                Ok(SameTextTarget {
                    id: row.get(0)?,
                    content_type: row.get(1)?,
                    content_hash: row.get(2)?,
                    preview: row.get(3)?,
                })
            })
            .optional()?;
        Ok(target)
    }

    pub(crate) fn merge_same_text(
        &self,
        target: &SameTextTarget,
        input: &MergeInput<'_>,
        images_dir: Option<&std::path::Path>,
    ) -> Result<i64> {
        let preview = Some(
            target
                .preview
                .clone()
                .unwrap_or_else(|| crate::truncate_preview(input.text)),
        );
        let content_type =
            crate::same_text::upgraded_type(&target.content_type, input.content_type);
        let categories =
            crate::ContentCategory::membership(Some(content_type), Some(input.text), &[]);
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        // 本次缺省的表示保留已有列，避免纯文本/单格式回写清空另一侧富文本。
        let (existing_html, existing_rtf): (Option<String>, Option<String>) = tx.query_row(
            "SELECT html_content, rtf_content FROM clipboard_items WHERE id = ?1",
            [target.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let html = input.html.map(str::to_owned).or(existing_html);
        let rtf_encoded = input
            .rtf
            .map(crate::rich::encode_rtf_for_storage)
            .or(existing_rtf);
        let byte_size = input.text.len()
            + html.as_ref().map_or(0, |value| value.len())
            + rtf_encoded.as_ref().map_or(0, |value| value.len());
        tx.execute(
            "UPDATE clipboard_items SET html_content = ?1, rtf_content = ?2,
             content_type = ?3, categories = ?4, byte_size = ?5, preview = ?6,
             access_count = access_count + 1,
             last_accessed_at = datetime('now', 'localtime'),
             updated_at = datetime('now', 'localtime'),
             sort_order = (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_items)
             WHERE id = ?7",
            rusqlite::params![
                html,
                rtf_encoded,
                content_type.as_str(),
                categories,
                byte_size as i64,
                preview,
                target.id
            ],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO content_hash_aliases (alias_hash, canonical_hash)
             VALUES (?1, ?2)",
            rusqlite::params![input.hash, target.content_hash],
        )?;
        tx.commit()?;
        drop(connection);
        self.enforce_history_limit(images_dir)?;
        Ok(target.id)
    }
}
