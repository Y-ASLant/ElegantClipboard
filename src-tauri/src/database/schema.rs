/// Database schema SQL
pub const SCHEMA_SQL: &str = r#"
-- Clipboard items table
CREATE TABLE IF NOT EXISTS clipboard_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type TEXT NOT NULL CHECK(content_type IN ('text', 'image', 'html', 'rtf', 'files')),
    text_content TEXT,
    html_content TEXT,
    rtf_content TEXT,
    image_path TEXT,
    file_paths TEXT,
    content_hash TEXT NOT NULL UNIQUE,
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
    last_accessed_at TEXT
);

-- Settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now', 'localtime'))
);

-- Full-text search index
CREATE VIRTUAL TABLE IF NOT EXISTS clipboard_fts USING fts5(
    text_content,
    preview,
    content='clipboard_items',
    content_rowid='id',
    tokenize='unicode61'
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER IF NOT EXISTS clipboard_items_ai AFTER INSERT ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(rowid, text_content, preview)
    VALUES (new.id, new.text_content, new.preview);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_items_ad AFTER DELETE ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, text_content, preview)
    VALUES ('delete', old.id, old.text_content, old.preview);
END;

CREATE TRIGGER IF NOT EXISTS clipboard_items_au AFTER UPDATE ON clipboard_items BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, text_content, preview)
    VALUES ('delete', old.id, old.text_content, old.preview);
    INSERT INTO clipboard_fts(rowid, text_content, preview)
    VALUES (new.id, new.text_content, new.preview);
END;

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
CREATE INDEX IF NOT EXISTS idx_clipboard_access ON clipboard_items(access_count DESC, last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_clipboard_sort_order ON clipboard_items(sort_order DESC);

-- Insert default settings
INSERT OR IGNORE INTO settings (key, value) VALUES 
    ('hotkey', 'Ctrl+Shift+V'),
    ('max_history_count', '10000'),
    ('max_content_size_kb', '1024'),
    ('auto_start', 'true'),
    ('theme', 'system'),
    ('language', 'zh-CN'),
    ('save_images', 'true'),
    ('save_html', 'true'),
    ('save_rtf', 'false');
"#;

/// Content types enum
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

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "text" => Some(ContentType::Text),
            "image" => Some(ContentType::Image),
            "html" => Some(ContentType::Html),
            "rtf" => Some(ContentType::Rtf),
            "files" => Some(ContentType::Files),
            _ => None,
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
