pub const SCHEMA_SQL: &str = r#"
-- Custom groups table (must be created before clipboard_items due to FK)
CREATE TABLE IF NOT EXISTS groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    sort_order INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now', 'localtime'))
);

-- Clipboard items table
-- group_id IS NULL  => default group (ungrouped)
-- group_id = <id>  => belongs to that custom group
CREATE TABLE IF NOT EXISTS clipboard_items (
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
-- Per-group hash index: duplicates are allowed when dedup strategy is "always_new"
CREATE INDEX IF NOT EXISTS idx_clipboard_hash_default ON clipboard_items(content_hash) WHERE group_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_clipboard_hash_group ON clipboard_items(group_id, content_hash) WHERE group_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_default ON clipboard_items(semantic_hash) WHERE group_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_clipboard_semantic_hash_group ON clipboard_items(group_id, semantic_hash) WHERE group_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_favorite_order ON clipboard_items(favorite_order DESC) WHERE is_favorite = 1;
CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_group ON clipboard_items(group_id);

-- Insert default settings
INSERT OR IGNORE INTO settings (key, value) VALUES
    ('global_shortcut', 'Alt+C'),
    ('max_history_count', '10000'),
    ('max_content_size_kb', '1024'),
    ('dedup_strategy', 'move_to_top'),
    ('text_dedup_mode', 'semantic'),
    ('autostart_enabled', 'false'),
    ('theme', 'system'),
    ('language', 'zh-CN'),
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
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::Html => "html",
            ContentType::Rtf => "rtf",
            ContentType::Files => "files",
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
