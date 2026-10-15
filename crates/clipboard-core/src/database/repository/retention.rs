use super::{ClipboardRepository, ConditionBuilder};
use rusqlite::params_from_iter;
use tracing::debug;

impl ClipboardRepository {
    /// 获取可清除条目的图片路径（按分组/类型过滤）
    pub fn get_clearable_image_paths(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .condition("image_path IS NOT NULL")
            .select_strings(&conn, "SELECT image_path FROM clipboard_items")
    }

    /// 获取可清除条目的 file_payload（按分组/类型过滤）
    pub fn get_clearable_file_payloads(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .condition("file_payload IS NOT NULL")
            .select_strings(&conn, "SELECT file_payload FROM clipboard_items")
    }

    /// 清空历史（保留置顶和收藏），按分组/类型过滤
    pub fn clear_history(
        &self,
        group_id: Option<i64>,
        content_type: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .content_type(content_type)
            .delete_items(&conn)
    }

    /// 获取所有条目的图片路径（含置顶和收藏）
    pub fn get_all_image_paths(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt =
            conn.prepare("SELECT image_path FROM clipboard_items WHERE image_path IS NOT NULL")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
    }

    /// 获取所有条目的 file_payload（含置顶和收藏）
    pub fn get_all_file_payloads(&self) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.read_conn.lock();
        let mut stmt = conn
            .prepare("SELECT file_payload FROM clipboard_items WHERE file_payload IS NOT NULL")?;
        stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
    }

    /// 清空所有历史（包括置顶和收藏）
    pub fn clear_all(&self) -> Result<i64, rusqlite::Error> {
        let conn = self.write_conn.lock();
        let deleted = conn.execute("DELETE FROM clipboard_items", [])?;
        Ok(deleted as i64)
    }

    /// 执行最大数量限制（按分组），返回 (删除数, 图片路径, file_payload)
    pub fn enforce_max_count(
        &self,
        max_count: i64,
        group_id: Option<i64>,
    ) -> Result<(i64, Vec<String>, Vec<String>), rusqlite::Error> {
        if max_count <= 0 {
            return Ok((0, vec![], vec![]));
        }

        let conn = self.write_conn.lock();
        let current_count = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .count_items(&conn)?;

        if current_count <= max_count {
            return Ok((0, vec![], vec![]));
        }
        let to_delete = current_count - max_count;

        let candidates = ConditionBuilder::new()
            .clearable()
            .group(group_id)
            .param(to_delete);
        let selector = format!(
            "SELECT id FROM clipboard_items{} ORDER BY created_at ASC, id ASC LIMIT ?",
            candidates.where_clause()
        );
        let mut image_paths = Vec::new();
        let mut file_payloads = Vec::new();
        {
            let sql = format!(
                "SELECT image_path, file_payload FROM clipboard_items WHERE id IN ({selector})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(&candidates.params), |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })?;
            for row in rows {
                let (image, payload) = row?;
                image_paths.extend(image);
                file_payloads.extend(payload);
            }
        }
        let deleted = conn.execute(
            &format!("DELETE FROM clipboard_items WHERE id IN ({selector})"),
            params_from_iter(&candidates.params),
        )? as i64;

        debug!(
            "Enforced max count: deleted {} oldest items (group: {:?})",
            deleted, group_id
        );
        Ok((deleted, image_paths, file_payloads))
    }
}
