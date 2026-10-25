use crate::{DeletionOutcome, History, preferences::RETENTION_KEY};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Limits apply independently to each group's unpinned, non-favorite items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetentionPolicy {
    /// Zero disables age-based removal. Age follows the stored local creation timestamp.
    pub days: u32,
    /// Zero keeps an unlimited number of items, independently of the UI page limit.
    pub max_count: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            days: 0,
            max_count: 10_000,
        }
    }
}

impl RetentionPolicy {
    pub fn requires_confirmation(self, previous: Self) -> bool {
        (self.days > 0 && (previous.days == 0 || self.days < previous.days))
            || (self.max_count > 0
                && (previous.max_count == 0 || self.max_count < previous.max_count))
    }
}

impl History {
    /// Persist and apply a confirmed policy atomically. On SQL failure neither changes.
    pub fn save_retention(
        &self,
        policy: RetentionPolicy,
        confirmed: bool,
        images_dir: &Path,
    ) -> Result<DeletionOutcome> {
        self.retention_transaction(Some((policy, confirmed)), Some(images_dir), false)
    }

    /// Startup and periodic maintenance only apply an explicitly saved policy.
    pub fn apply_retention(&self, images_dir: &Path) -> Result<DeletionOutcome> {
        self.retention_transaction(None, Some(images_dir), false)
    }

    pub(crate) fn enforce_history_limit(&self, images_dir: Option<&Path>) -> Result<()> {
        let outcome = self.retention_transaction(None, images_dir, true)?;
        if let Some(warning) = outcome.cleanup_warning {
            anyhow::bail!("历史已保存并清理，但附件回收不完整：{warning}");
        }
        Ok(())
    }

    fn retention_transaction(
        &self,
        change: Option<(RetentionPolicy, bool)>,
        images_dir: Option<&Path>,
        capture: bool,
    ) -> Result<DeletionOutcome> {
        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let stored: Option<String> = tx
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [RETENTION_KEY],
                |row| row.get(0),
            )
            .optional()?;
        // Before the first explicit save, preserve the original capture-only default-group limit.
        let legacy_scope = stored.is_none() && change.is_none();
        if legacy_scope && !capture {
            return Ok(DeletionOutcome {
                deleted: 0,
                cleanup_warning: None,
            });
        }
        // Invalid persisted policy is an error: never fall back to a destructive default.
        let previous: RetentionPolicy = stored
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        let policy = if let Some((policy, confirmed)) = change {
            ensure!(
                confirmed || (stored.is_some() && !policy.requires_confirmation(previous)),
                "首次应用到全部分组或收紧保留限制需要确认"
            );
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at)
                 VALUES (?1, ?2, datetime('now', 'localtime'))",
                params![RETENTION_KEY, serde_json::to_string(&policy)?],
            )?;
            policy
        } else {
            previous
        };
        let mut images = Vec::new();
        let mut payloads = Vec::new();
        let mut ids = Vec::new();
        let mut hashes = Vec::new();
        if policy.days > 0 || policy.max_count > 0 {
            let mut query = tx.prepare(
                "WITH ranked AS (
                    SELECT id, image_path, file_payload, content_hash, created_at,
                           ROW_NUMBER() OVER (PARTITION BY group_id ORDER BY created_at DESC, id DESC) AS position
                    FROM clipboard_items WHERE is_pinned = 0 AND is_favorite = 0
                        AND (?3 = 0 OR group_id IS NULL)
                 )
                 SELECT id, image_path, file_payload, content_hash FROM ranked
                 WHERE (?1 > 0 AND julianday(created_at) < julianday('now', 'localtime') - ?1)
                    OR (?2 > 0 AND position > ?2)",
            )?;
            let rows = query.query_map(
                params![policy.days, policy.max_count, legacy_scope],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            for row in rows {
                let (id, image, payload, hash) = row?;
                ids.push(id);
                images.extend(image);
                payloads.extend(payload);
                hashes.push(hash);
            }
        }
        let blobs = crate::native_format::clear_native_by_ids(&tx, &ids, &hashes)?;
        {
            let mut delete = tx.prepare("DELETE FROM clipboard_items WHERE id = ?1")?;
            for id in &ids {
                delete.execute([id])?;
            }
        }
        tx.commit()?;
        drop(connection);
        Ok(DeletionOutcome {
            deleted: ids.len() as i64,
            cleanup_warning: images_dir
                .and_then(|root| self.cleanup_media(images, payloads, blobs, root)),
        })
    }
}

#[cfg(test)]
mod tests;
