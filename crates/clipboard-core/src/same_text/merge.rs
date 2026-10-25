use crate::History;
use crate::database::ClipboardItem;
use crate::same_text::MergeInput;
use anyhow::Result;

impl History {
    pub(crate) fn find_same_text_item(&self, text: &str) -> Result<Option<ClipboardItem>> {
        Ok(self.repo.find_same_text_item(text)?)
    }

    pub(crate) fn merge_same_text(
        &self,
        target: &ClipboardItem,
        input: &MergeInput<'_>,
        images_dir: Option<&std::path::Path>,
    ) -> Result<i64> {
        let rtf_encoded = input.rtf.map(crate::rich::encode_rtf_for_storage);
        let html = input.html.map(str::to_owned);
        let text = input.text.map(str::to_owned);
        let next_type = crate::same_text::upgraded_type(&target.content_type, input.content_type);
        let byte_size = text.as_ref().map_or(0, |value| value.len())
            + html.as_ref().map_or(0, |value| value.len())
            + rtf_encoded.as_ref().map_or(0, |value| value.len());
        let preview = target.preview.clone().or_else(|| {
            text.as_ref()
                .map(|value| value.chars().take(240).collect::<String>())
        });
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
                next_type.as_str(),
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
