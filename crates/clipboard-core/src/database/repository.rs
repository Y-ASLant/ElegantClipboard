use super::{ContentType, Database};
use parking_lot::Mutex;
use rusqlite::{Connection, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod mutations;
mod query;
mod retention;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: i64,
    pub content_type: String,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<String>,
    /// 文件剪贴板 fidelity payload（CF_HDROP + 伴生格式 + staging）
    pub file_payload: Option<String>,
    pub content_hash: String,
    pub semantic_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub is_pinned: bool,
    pub is_favorite: bool,
    pub favorite_order: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: i64,
    pub last_accessed_at: Option<String>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
    /// 用户备注（NULL = 无备注；参与关键词搜索）
    pub note: Option<String>,
    /// 所属分组（NULL = 默认分组，Some(id) = 自定义分组）
    pub group_id: Option<i64>,
}

impl ClipboardItem {
    /// 存储的类型串解析为枚举；未知值不作为纯文本处理。
    pub fn kind(&self) -> Option<ContentType> {
        ContentType::parse(&self.content_type)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewClipboardItem {
    pub content_type: ContentType,
    pub text_content: Option<String>,
    pub html_content: Option<String>,
    pub rtf_content: Option<String>,
    pub image_path: Option<String>,
    pub file_paths: Option<Vec<String>>,
    pub file_payload: Option<String>,
    pub content_hash: String,
    pub semantic_hash: String,
    pub preview: Option<String>,
    pub byte_size: i64,
    pub image_width: Option<i64>,
    pub image_height: Option<i64>,
    pub char_count: Option<i64>,
    pub source_app_name: Option<String>,
    pub source_app_icon: Option<String>,
    /// None = 默认分组，Some(id) = 自定义分组
    pub group_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryOptions {
    pub search: Option<String>,
    pub content_type: Option<String>,
    pub favorite_only: bool,
    pub group_id: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub item_count: i64,
}

/// 剪贴板条目仓库（读写分离）
pub struct ClipboardRepository {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
}

/// Dynamic SQL conditions and owned SQLite parameters.
struct ConditionBuilder {
    conditions: Vec<String>,
    params: Vec<Value>,
}

impl ConditionBuilder {
    fn new() -> Self {
        Self {
            conditions: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Search uses the first parameter for both columns; add other filters afterward.
    fn filtered(options: &QueryOptions) -> Self {
        let mut builder = Self::new();
        if let Some(search) = options
            .search
            .as_deref()
            .filter(|search| !search.is_empty())
        {
            let mut pattern = String::with_capacity(search.len() + 2);
            pattern.push('%');
            for ch in search.chars() {
                if matches!(ch, '\\' | '%' | '_') {
                    pattern.push('\\');
                }
                pattern.push(ch);
            }
            pattern.push('%');
            builder = builder.condition_with_param(
                "(text_content LIKE ?1 ESCAPE '\\' OR file_paths LIKE ?1 ESCAPE '\\' \
                 OR note LIKE ?1 ESCAPE '\\')",
                pattern,
            );
        }
        builder = builder.content_type(options.content_type.as_deref());
        if options.favorite_only {
            builder = builder.condition("is_favorite = 1");
        }
        builder.group(options.group_id)
    }

    /// Add non-pinned + non-favorite conditions (clearable items).
    fn clearable(mut self) -> Self {
        self.conditions.push("is_pinned = 0".to_string());
        self.conditions.push("is_favorite = 0".to_string());
        self
    }

    /// Add group_id filter (NULL = default group, Some(id) = custom group).
    fn group(mut self, group_id: Option<i64>) -> Self {
        let (cond, param) = ClipboardRepository::group_condition(group_id);
        self.conditions.push(cond.to_string());
        if let Some(gid) = param {
            self.params.push(gid.into());
        }
        self
    }

    /// Add content_type filter (supports comma-separated multi-type).
    fn content_type(mut self, content_type: Option<&str>) -> Self {
        ClipboardRepository::append_content_type_condition(
            content_type,
            &mut self.conditions,
            &mut self.params,
        );
        self
    }

    /// Add a condition without an associated parameter.
    fn condition(mut self, cond: &str) -> Self {
        self.conditions.push(cond.to_string());
        self
    }

    /// Add a condition with an associated parameter.
    fn condition_with_param(mut self, cond: &str, value: impl Into<Value>) -> Self {
        self.conditions.push(cond.to_string());
        self.params.push(value.into());
        self
    }

    /// Push a trailing parameter (e.g., for LIMIT clauses outside WHERE).
    fn param(mut self, value: impl Into<Value>) -> Self {
        self.params.push(value.into());
        self
    }

    /// Build the WHERE clause (includes leading ` WHERE `).
    fn where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// SELECT single-column string results.
    fn select_strings(
        &self,
        conn: &Connection,
        prefix: &str,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let sql = format!("{}{}", prefix, self.where_clause());
        let mut stmt = conn.prepare(&sql)?;
        stmt.query_map(params_from_iter(&self.params), |row| {
            row.get::<_, String>(0)
        })?
        .collect()
    }

    /// DELETE FROM clipboard_items and return affected row count.
    fn delete_items(&self, conn: &Connection) -> Result<i64, rusqlite::Error> {
        let sql = format!("DELETE FROM clipboard_items{}", self.where_clause());
        Ok(conn.execute(&sql, params_from_iter(&self.params))? as i64)
    }

    /// COUNT(*) on clipboard_items.
    fn count_items(&self, conn: &Connection) -> Result<i64, rusqlite::Error> {
        let sql = format!(
            "SELECT COUNT(*) FROM clipboard_items{}",
            self.where_clause()
        );
        conn.query_row(&sql, params_from_iter(&self.params), |row| row.get(0))
    }
}

impl ClipboardRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            write_conn: db.write_connection(),
            read_conn: db.read_connection(),
        }
    }

    /// 将 group_id 转换为 SQL 条件片段和可选参数
    fn group_condition(group_id: Option<i64>) -> (&'static str, Option<i64>) {
        match group_id {
            Some(gid) => ("group_id = ?", Some(gid)),
            None => ("group_id IS NULL", None),
        }
    }

    /// 将 content_type（支持逗号分隔）转换为 SQL 条件并追加参数。
    fn append_content_type_condition(
        content_type: Option<&str>,
        conditions: &mut Vec<String>,
        params_vec: &mut Vec<Value>,
    ) {
        let Some(raw) = content_type else {
            return;
        };
        let types: Vec<&str> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if types.is_empty() {
            return;
        }
        if types.len() == 1 {
            conditions.push("content_type = ?".to_string());
            params_vec.push(types[0].to_string().into());
        } else {
            let placeholders: Vec<&str> = types.iter().map(|_| "?").collect();
            conditions.push(format!("content_type IN ({})", placeholders.join(",")));
            for t in &types {
                params_vec.push((*t).to_string().into());
            }
        }
    }
}

#[cfg(test)]
mod tests;
