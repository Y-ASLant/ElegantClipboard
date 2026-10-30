use crate::{History, native_format::valid_format_name};
use anyhow::{Result, bail};
use rusqlite::params;

const MAX_CAPTURED_FORMATS: usize = 4096;

impl History {
    /// Replace the names observed in the latest clipboard snapshot, independently of payloads.
    /// Empty names mean unknown. Exact native spelling is preserved; duplicates are removed.
    pub fn set_captured_formats(&self, item_id: i64, names: &[String]) -> Result<()> {
        if names.len() > MAX_CAPTURED_FORMATS {
            bail!("剪贴板格式名称数量超过上限");
        }
        if names
            .iter()
            .any(|name| !valid_format_name(name) || name.trim().is_empty())
        {
            bail!("剪贴板格式名称无效");
        }
        let mut names: Vec<&str> = names.iter().map(String::as_str).collect();
        names.sort_unstable();
        names.dedup();

        let connection = self.db.write_connection();
        let mut connection = connection.lock();
        let tx = connection.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM clipboard_items WHERE id = ?1)",
            [item_id],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("记录已不存在，无法保存采集格式名称");
        }
        tx.execute("DELETE FROM captured_formats WHERE item_id = ?1", [item_id])?;
        {
            let mut insert =
                tx.prepare("INSERT INTO captured_formats (item_id, format_name) VALUES (?1, ?2)")?;
            for name in names {
                insert.execute(params![item_id, name])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
