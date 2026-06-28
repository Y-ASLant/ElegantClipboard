mod repository;
mod schema;

pub use repository::*;
pub use schema::*;

use crate::clipboard::{compute_semantic_hash, is_url};
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, params};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

/// 数据库管理器（读写分离）
pub struct Database {
    write_conn: Arc<Mutex<Connection>>,
    read_conn: Arc<Mutex<Connection>>,
    db_path: PathBuf,
}

impl Database {
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let write_conn = Connection::open(&db_path)?;
        Self::configure_connection(&write_conn, false)?;

        let read_conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        Self::configure_connection(&read_conn, true)?;

        info!("Database opened at {:?}", db_path);

        let db = Self {
            write_conn: Arc::new(Mutex::new(write_conn)),
            read_conn: Arc::new(Mutex::new(read_conn)),
            db_path,
        };

        db.init_schema()?;

        Ok(db)
    }

    fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
        if read_only {
            conn.execute_batch(
                "PRAGMA query_only = ON;
                 PRAGMA cache_size = -32000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;",
            )?;
        } else {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = -64000;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 268435456;
                 PRAGMA foreign_keys = ON;",
            )?;
        }
        Ok(())
    }

    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();

        Self::run_migrations(&conn)?;

        conn.execute_batch(SCHEMA_SQL)?;
        info!("Database schema initialized");

        Ok(())
    }

    /// 数据库迁移（在 schema 创建前执行）
    fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !table_exists {
            return Ok(());
        }

        Self::recover_orphan_clipboard_items_table(conn)?;

        // 迁移 1: sort_order
        let has_sort_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'sort_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_sort_order {
            info!("Migrating database: adding sort_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN sort_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items SET sort_order = id;",
            )?;
            info!("Migration complete: sort_order column added");
        }

        // 迁移 2: 移除 FTS5（改用 LIKE 支持中文搜索）
        let has_fts: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_fts'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if has_fts {
            info!("Migrating database: removing FTS5 table and triggers");
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS clipboard_items_ai;
                 DROP TRIGGER IF EXISTS clipboard_items_ad;
                 DROP TRIGGER IF EXISTS clipboard_items_au;
                 DROP TABLE IF EXISTS clipboard_fts;",
            )?;
            info!("Migration complete: FTS5 removed");
        }

        // 迁移 3: char_count
        let has_char_count: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'char_count'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_char_count {
            info!("Migrating database: adding char_count column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN char_count INTEGER;
                 UPDATE clipboard_items SET char_count = LENGTH(text_content) WHERE text_content IS NOT NULL;"
            )?;
            info!("Migration complete: char_count column added");
        }

        // 迁移 4: image_width/image_height
        let has_image_width: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'image_width'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_image_width {
            info!("Migrating database: adding image_width and image_height columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN image_width INTEGER;
                 ALTER TABLE clipboard_items ADD COLUMN image_height INTEGER;",
            )?;
            info!("Migration complete: image_width and image_height columns added");
        }

        // 迁移 5: source_app_name/source_app_icon
        let has_source_app: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'source_app_name'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_source_app {
            info!("Migrating database: adding source_app_name and source_app_icon columns");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN source_app_name TEXT;
                 ALTER TABLE clipboard_items ADD COLUMN source_app_icon TEXT;",
            )?;
            info!("Migration complete: source_app columns added");
        }

        // 迁移 6: 添加 group_id 并将 content_hash 唯一性改为分组内唯一（重建表）
        let has_group_id: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'group_id'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_group_id {
            info!("Migrating database: adding group_id column (table rebuild)");

            // 确认 item_groups 表是否存在（用于迁移旧分组关联数据）
            let has_item_groups: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='item_groups'",
                [],
                |row| row.get(0),
            ).unwrap_or(false);

            let tx = conn.unchecked_transaction()?;

            // 先确保 groups 表存在（schema 顺序已调整，但旧库可能没有）
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );",
            )?;

            // 建新表（含 group_id）
            tx.execute_batch(
                "CREATE TABLE clipboard_items_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT NOT NULL,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );"
            )?;

            // 复制数据：若 item_groups 存在则从中取 MIN(group_id)，否则全部设为 NULL（默认分组）
            if has_item_groups {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite,
                            CASE WHEN is_favorite = 1 THEN sort_order ELSE 0 END, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon,
                            (SELECT MIN(ig.group_id) FROM item_groups ig WHERE ig.item_id = clipboard_items.id)
                     FROM clipboard_items;"
                )?;
            } else {
                tx.execute_batch(
                    "INSERT INTO clipboard_items_new 
                     SELECT id, content_type, text_content, html_content, rtf_content,
                            image_path, file_paths, content_hash, content_hash, preview, byte_size,
                            image_width, image_height, is_pinned, is_favorite,
                            CASE WHEN is_favorite = 1 THEN sort_order ELSE 0 END, sort_order,
                            created_at, updated_at, access_count, last_accessed_at, char_count,
                            source_app_name, source_app_icon, NULL
                     FROM clipboard_items;",
                )?;
            }

            // 处理重复 hash（同一分组内可能有多条相同 hash 的记录，保留最新一条）
            tx.execute_batch(
                "DELETE FROM clipboard_items_new WHERE id NOT IN (
                    SELECT MAX(id) FROM clipboard_items_new
                    GROUP BY COALESCE(CAST(group_id AS TEXT), 'NULL'), content_hash
                );",
            )?;

            // 删除旧表并重命名
            tx.execute_batch(
                "DROP TABLE clipboard_items;
                 ALTER TABLE clipboard_items_new RENAME TO clipboard_items;
                 DROP TABLE IF EXISTS item_groups;
                 -- 重建索引
                 CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
                 CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
                 -- 重建触发器
                 CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
                 AFTER UPDATE ON clipboard_items
                 BEGIN
                     UPDATE clipboard_items SET updated_at = datetime('now', 'localtime') WHERE id = new.id;
                 END;"
            )?;

            tx.commit()?;
            info!("Migration complete: group_id column added, table rebuilt");
        }

        // 迁移 7: favorite_order
        let has_favorite_order: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'favorite_order'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_favorite_order {
            info!("Migrating database: adding favorite_order column");
            conn.execute_batch(
                "ALTER TABLE clipboard_items ADD COLUMN favorite_order INTEGER DEFAULT 0;
                 UPDATE clipboard_items
                 SET favorite_order = sort_order
                 WHERE is_favorite = 1;
                 CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order
                   ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;",
            )?;
            info!("Migration complete: favorite_order column added");
        }

        // 迁移 8: 允许重复 content_hash（always_new 策略），保留索引但移除唯一约束
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_clipboard_hash_default;
             DROP INDEX IF EXISTS idx_clipboard_hash_group;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default
               ON clipboard_items(content_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group
               ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;",
        )?;

        // Migration 8: add semantic_hash and backfill existing rows.
        let has_semantic_hash: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('clipboard_items') WHERE name = 'semantic_hash'",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !has_semantic_hash {
            info!("Migrating database: adding semantic_hash column");
            conn.execute_batch("ALTER TABLE clipboard_items ADD COLUMN semantic_hash TEXT;")?;
        }

        Self::backfill_semantic_hashes(conn)?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default
               ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group
               ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;",
        )?;

        // 迁移 9: 修复 issue #81 收藏顺序乱跳的根因——存量数据中存在重复或零值
        // favorite_order，导致收藏列表排序退化为 sort_order/created_at 等次级键，
        // 进而被复制/粘贴动作引发的 bump_to_top / touch_by_column 间接打乱。
        // 这里按用户当前看到的顺序 (favorite_order DESC, id DESC) 重新分配单调递增、
        // 互不相同的整数；toggle_favorite / move_favorite_item_by_id 已天然保证
        // favorite_order 唯一，所以一次规整后就不会再退化。
        Self::normalize_favorite_order(conn)?;

        // 迁移 10: 新增 url 内容类型，并将存量单行链接从 text 归类为 url
        Self::migrate_url_content_type(conn)?;

        Ok(())
    }

    /// 若迁移过程中进程异常退出，可能留下 clipboard_items_new 而无 clipboard_items
    fn recover_orphan_clipboard_items_table(conn: &Connection) -> Result<(), rusqlite::Error> {
        let has_new: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items_new'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        let has_old: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if has_new && !has_old {
            tracing::warn!("Recovering orphaned clipboard_items_new from interrupted migration");
            conn.execute_batch("ALTER TABLE clipboard_items_new RENAME TO clipboard_items;")?;
        }
        Ok(())
    }

    fn migrate_url_content_type(conn: &Connection) -> Result<(), rusqlite::Error> {
        let table_sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .ok();
        let Some(table_sql) = table_sql else {
            return Ok(());
        };
        if table_sql.contains("'url'") {
            return Ok(());
        }

        info!("Migrating database: adding url content_type");
        // 迁移 8 可能留下 NULL semantic_hash；重建表要求 NOT NULL，先补齐
        conn.execute_batch(
            "UPDATE clipboard_items
             SET semantic_hash = content_hash
             WHERE semantic_hash IS NULL OR semantic_hash = '';",
        )?;

        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(
            "CREATE TABLE clipboard_items_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'url')),
                text_content TEXT,
                html_content TEXT,
                rtf_content TEXT,
                image_path TEXT,
                file_paths TEXT,
                content_hash TEXT NOT NULL,
                semantic_hash TEXT NOT NULL,
                preview TEXT,
                byte_size INTEGER DEFAULT 0,
                image_width INTEGER,
                image_height INTEGER,
                is_pinned INTEGER DEFAULT 0,
                is_favorite INTEGER DEFAULT 0,
                favorite_order INTEGER DEFAULT 0,
                sort_order INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now', 'localtime')),
                updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                access_count INTEGER DEFAULT 0,
                last_accessed_at TEXT,
                char_count INTEGER,
                source_app_name TEXT,
                source_app_icon TEXT,
                group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
            );
            INSERT INTO clipboard_items_new
            SELECT id, content_type, text_content, html_content, rtf_content,
                   image_path, file_paths, content_hash,
                   COALESCE(NULLIF(semantic_hash, ''), content_hash),
                   preview, byte_size, image_width, image_height,
                   is_pinned, is_favorite, favorite_order, sort_order,
                   created_at, updated_at, access_count, last_accessed_at,
                   char_count, source_app_name, source_app_icon, group_id
            FROM clipboard_items;
            DROP TABLE clipboard_items;
            ALTER TABLE clipboard_items_new RENAME TO clipboard_items;",
        )?;
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
             CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
             CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);
             CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp
             AFTER UPDATE ON clipboard_items
             BEGIN
                 UPDATE clipboard_items SET updated_at = datetime('now', 'localtime')
                 WHERE id = new.id;
             END;",
        )?;
        tx.commit()?;

        Self::backfill_url_content_types(conn)?;
        info!("Migration complete: url content_type added");
        Ok(())
    }

    fn backfill_url_content_types(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, text_content FROM clipboard_items WHERE content_type = 'text' AND text_content IS NOT NULL",
        )?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(std::result::Result::ok)
            .collect();

        let mut update =
            conn.prepare_cached("UPDATE clipboard_items SET content_type = 'url' WHERE id = ?1")?;
        let mut count = 0;
        for (id, text) in rows {
            if is_url(&text) {
                update.execute(params![id])?;
                count += 1;
            }
        }
        if count > 0 {
            info!("Backfilled {} url content types", count);
        }
        Ok(())
    }

    /// 将所有 `is_favorite = 1` 的条目的 `favorite_order` 规整为
    /// 单调递增、互不相同的整数，保留当前显示顺序。
    /// 当不存在重复或零值时直接跳过，避免无谓写入。
    fn normalize_favorite_order(conn: &Connection) -> Result<(), rusqlite::Error> {
        // 是否存在并列或为零的 favorite_order
        let needs_fix: bool = conn.query_row(
            "SELECT (COUNT(*) > COUNT(DISTINCT favorite_order)) \
             OR EXISTS(SELECT 1 FROM clipboard_items WHERE is_favorite = 1 AND favorite_order <= 0) \
             FROM clipboard_items WHERE is_favorite = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(false);

        if !needs_fix {
            return Ok(());
        }

        // 取出所有收藏项 id，按当前显示顺序排列（最前面的在最前）
        let ids: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM clipboard_items \
                 WHERE is_favorite = 1 \
                 ORDER BY favorite_order DESC, id DESC",
            )?;
            stmt.query_map([], |row| row.get::<_, i64>(0))?
                .filter_map(std::result::Result::ok)
                .collect()
        };

        if ids.is_empty() {
            return Ok(());
        }

        info!(
            "Migrating database: normalizing favorite_order for {} items",
            ids.len()
        );

        let total = ids.len() as i64;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE clipboard_items SET favorite_order = ?1 \
                 WHERE id = ?2 AND is_favorite = 1",
            )?;
            for (idx, id) in ids.iter().enumerate() {
                let new_order = total - idx as i64; // 最前面的得到最大值
                stmt.execute(params![new_order, id])?;
            }
        }
        tx.commit()?;

        info!("Migration complete: favorite_order normalized");
        Ok(())
    }

    fn backfill_semantic_hashes(conn: &Connection) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, content_type, text_content, content_hash, semantic_hash
             FROM clipboard_items
             WHERE semantic_hash IS NULL
                OR semantic_hash = ''
                OR (content_type IN ('text', 'html', 'rtf') AND semantic_hash = content_hash)",
        )?;

        let mut updates: Vec<(i64, String)> = Vec::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;

        for row in rows {
            let (id, content_type, text_content, content_hash, existing_semantic_hash) = row?;
            let computed_semantic_hash =
                compute_semantic_hash(&content_type, text_content.as_deref(), &content_hash);

            if existing_semantic_hash.as_deref() != Some(computed_semantic_hash.as_str()) {
                updates.push((id, computed_semantic_hash));
            }
        }
        drop(stmt);

        if updates.is_empty() {
            return Ok(());
        }

        let updated_count = updates.len();
        let tx = conn.unchecked_transaction()?;
        {
            let mut update_stmt =
                tx.prepare("UPDATE clipboard_items SET semantic_hash = ?1 WHERE id = ?2")?;
            for (id, semantic_hash) in updates {
                update_stmt.execute(params![semantic_hash, id])?;
            }
        }
        tx.commit()?;
        info!(
            "Migration complete: semantic_hash backfilled for {} rows",
            updated_count
        );
        Ok(())
    }

    pub fn write_connection(&self) -> Arc<Mutex<Connection>> {
        self.write_conn.clone()
    }

    pub fn read_connection(&self) -> Arc<Mutex<Connection>> {
        self.read_conn.clone()
    }

    pub fn optimize(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute_batch("PRAGMA optimize;")?;
        info!("Database optimized");
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let conn = self.write_conn.lock();
        conn.execute_batch("VACUUM;")?;
        info!("Database vacuumed");
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            write_conn: self.write_conn.clone(),
            read_conn: self.read_conn.clone(),
            db_path: self.db_path.clone(),
        }
    }
}

/// 获取应用安装目录（可执行文件所在目录）
pub fn get_app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn get_default_db_path() -> PathBuf {
    get_app_dir().join("clipboard.db")
}

pub fn get_default_images_path() -> PathBuf {
    get_app_dir().join("images")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> Database {
        let dir = std::env::temp_dir().join(format!("ec_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("test_{}.db", uuid_simple()));
        Database::new(path).unwrap()
    }

    fn uuid_simple() -> u64 {
        use std::time::SystemTime;
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[test]
    fn new_database_creates_schema() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn schema_creates_groups_table() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='groups'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn schema_creates_settings_table_with_defaults() {
        let db = temp_db();
        let conn = db.read_connection();
        let conn = conn.lock();
        let val: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'global_shortcut'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "Alt+C");
    }

    #[test]
    fn read_write_separation() {
        let db = temp_db();
        {
            let w = db.write_connection();
            let w = w.lock();
            w.execute(
                "INSERT INTO settings (key, value) VALUES ('test_rw', 'hello')",
                [],
            )
            .unwrap();
        }
        let r = db.read_connection();
        let r = r.lock();
        let val: String = r
            .query_row(
                "SELECT value FROM settings WHERE key = 'test_rw'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn optimize_and_vacuum() {
        let db = temp_db();
        assert!(db.optimize().is_ok());
        assert!(db.vacuum().is_ok());
    }

    #[test]
    fn clone_shares_connections() {
        let db = temp_db();
        let db2 = db.clone();
        {
            let w = db.write_connection();
            let w = w.lock();
            w.execute(
                "INSERT INTO settings (key, value) VALUES ('clone_test', 'shared')",
                [],
            )
            .unwrap();
        }
        let r = db2.read_connection();
        let r = r.lock();
        let val: String = r
            .query_row(
                "SELECT value FROM settings WHERE key = 'clone_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "shared");
    }

    #[test]
    fn migration_10_adds_url_content_type_from_legacy_schema() {
        let dir = std::env::temp_dir().join(format!("ec_mig10_{}", uuid_simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.db");

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime'))
                );
                CREATE TABLE clipboard_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
                    text_content TEXT,
                    html_content TEXT,
                    rtf_content TEXT,
                    image_path TEXT,
                    file_paths TEXT,
                    content_hash TEXT NOT NULL,
                    semantic_hash TEXT,
                    preview TEXT,
                    byte_size INTEGER DEFAULT 0,
                    image_width INTEGER,
                    image_height INTEGER,
                    is_pinned INTEGER DEFAULT 0,
                    is_favorite INTEGER DEFAULT 0,
                    favorite_order INTEGER DEFAULT 0,
                    sort_order INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
                    access_count INTEGER DEFAULT 0,
                    last_accessed_at TEXT,
                    char_count INTEGER,
                    source_app_name TEXT,
                    source_app_icon TEXT,
                    group_id INTEGER REFERENCES groups(id) ON DELETE CASCADE
                );
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT);
                INSERT INTO clipboard_items (
                    content_type, text_content, content_hash, semantic_hash, preview
                ) VALUES
                    ('text', 'https://example.com', 'hash_url', NULL, 'https://example.com'),
                    ('text', 'plain note', 'hash_text', '', 'plain note');",
            )
            .unwrap();
        }

        let db = Database::new(path).unwrap();
        let conn = db.read_connection();
        let conn = conn.lock();

        let table_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='clipboard_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_sql.contains("'url'"));

        let url_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE content_type = 'url'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(url_count, 1);

        let null_semantic: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clipboard_items WHERE semantic_hash IS NULL OR semantic_hash = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(null_semantic, 0);
    }
}
