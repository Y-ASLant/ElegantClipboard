use super::{FONT_KEY, FontPreference, Preferences};
use anyhow::Result;
use std::path::Path;

impl Preferences {
    pub fn fonts(&self) -> Result<FontPreference> {
        self.read_json(FONT_KEY, FontPreference::valid)
    }

    pub fn set_fonts(&self, preference: FontPreference) -> Result<()> {
        anyhow::ensure!(preference.valid(), "字体设置超出允许范围");
        self.write_json(FONT_KEY, &preference)
    }
}

/// 启动导入进度窗：只读短连接，避免随后删库被 WAL 占用。
pub fn peek_fonts(db_path: &Path) -> FontPreference {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};

    if !db_path.is_file() {
        return FontPreference::default();
    }
    let Ok(connection) = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return FontPreference::default();
    };
    let raw = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [FONT_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    raw.and_then(|value| serde_json::from_str(&value).ok())
        .filter(FontPreference::valid)
        .unwrap_or_default()
}
