use super::{FONT_KEY, FontPreference, Preferences};
use anyhow::Result;

impl Preferences {
    pub fn fonts(&self) -> Result<FontPreference> {
        self.read_json(FONT_KEY, FontPreference::valid)
    }

    pub fn set_fonts(&self, preference: FontPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "字体设置超出允许范围");
        self.write_json(FONT_KEY, &preference)
    }
}

/// 启动导入进度窗用：只读、短连接，不走 WAL 写连接，避免随后删库被占用。
pub fn peek_fonts(db_path: &std::path::Path) -> FontPreference {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};
    if !db_path.is_file() {
        return FontPreference::default();
    }
    (|| -> Result<FontPreference> {
        let connection = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [super::FONT_KEY],
                |row| row.get(0),
            )
            .optional()?;
        // 立刻关掉连接，释放文件共享锁。
        drop(connection);
        Ok(match raw {
            Some(value) => serde_json::from_str(&value).unwrap_or_default(),
            None => FontPreference::default(),
        })
    })()
    .unwrap_or_default()
}
