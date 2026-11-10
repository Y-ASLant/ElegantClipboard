use super::{ClipboardRepository, ConditionBuilder};

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
}
