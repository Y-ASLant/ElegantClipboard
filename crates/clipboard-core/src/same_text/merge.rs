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
               AND length(text_content) = ?2
             ORDER BY sort_order DESC, created_at DESC, id DESC LIMIT 1",
        )?;
        let target = statement
            .query_row(rusqlite::params![text, text.len() as i64], |row| {
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
        let rtf_encoded = input.rtf.map(crate::rich::encode_rtf_for_storage);
        let html = input.html.map(str::to_owned);
        let byte_size = input.text.len()
            + html.as_ref().map_or(0, |value| value.len())
            + rtf_encoded.as_ref().map_or(0, |value| value.len());
        let preview = Some(
            target
                .preview
                .clone()
                .unwrap_or_else(|| crate::truncate_preview(input.text)),
        );
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        tx.execute(
            "UPDATE clipboard_items SET html_content = ?1, rtf_content = ?2,
             content_type = ?3, byte_size = ?4, preview = ?5,
             access_count = access_count + 1,
             last_accessed_at = datetime('now', 'localtime'),
             updated_at = datetime('now', 'localtime'),
             sort_order = (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_items)
             WHERE id = ?6",
            rusqlite::params![
                html,
                rtf_encoded,
                crate::same_text::upgraded_type(&target.content_type, input.content_type).as_str(),
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
