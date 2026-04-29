pub const SCHEMA_SQL: &str = r#"
-- Tags table
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now', 'localtime'))
);

-- Item-Tag junction table (many-to-many)
CREATE TABLE IF NOT EXISTS item_tags (
    item_id INTEGER NOT NULL REFERENCES clipboard_items(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TEXT DEFAULT (datetime('now', 'localtime')),
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (item_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_item_tags_tag ON item_tags(tag_id);

-- Clipboard items table
CREATE TABLE IF NOT EXISTS clipboard_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files', 'video')),
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
    sort_order INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now', 'localtime')),
    updated_at TEXT DEFAULT (datetime('now', 'localtime')),
    access_count INTEGER DEFAULT 0,
    last_accessed_at TEXT,
    char_count INTEGER,
    source_app_name TEXT,
    source_app_icon TEXT,
    files_valid INTEGER DEFAULT 1
);

-- Settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now', 'localtime'))
);

-- Update timestamp trigger
CREATE TRIGGER IF NOT EXISTS clipboard_items_update_timestamp 
AFTER UPDATE ON clipboard_items
BEGIN
    UPDATE clipboard_items SET updated_at = datetime('now', 'localtime')
    WHERE id = new.id;
END;

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_clipboard_created_at ON clipboard_items(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_pinned ON clipboard_items(is_pinned) WHERE is_pinned = 1;
CREATE INDEX IF NOT EXISTS idx_clipboard_favorite ON clipboard_items(is_favorite) WHERE is_favorite = 1;
CREATE INDEX IF NOT EXISTS idx_clipboard_type ON clipboard_items(content_type);
CREATE INDEX IF NOT EXISTS idx_clipboard_hash ON clipboard_items(content_hash);
CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash ON clipboard_items(semantic_hash);
CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);

-- Insert default settings
INSERT OR IGNORE INTO settings (key, value) VALUES
    ('hotkey', 'Ctrl+Shift+V'),
    ('max_history_count', '10000'),
    ('max_content_size_kb', '1024'),
    ('dedup_strategy', 'move_to_top'),
    ('text_dedup_mode', 'semantic'),
    ('auto_start', 'false'),
    ('theme', 'system'),
    ('language', 'zh-CN'),
    ('save_images', 'true'),
    ('save_html', 'true'),
    ('save_rtf', 'false'),
    ('auto_cleanup_days', '30');
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Text,
    Image,
    Html,
    Rtf,
    Files,
    Video,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::Html => "html",
            ContentType::Rtf => "rtf",
            ContentType::Files => "files",
            ContentType::Video => "video",
        }
    }

}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
