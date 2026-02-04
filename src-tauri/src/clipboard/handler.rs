use crate::database::{
    get_images_path, ClipboardRepository, ContentType, Database, NewClipboardItem,
    SettingsRepository,
};
use blake3::Hasher;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Default maximum text content length (1MB)
const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;
/// Maximum preview length
const MAX_PREVIEW_LENGTH: usize = 200;
/// Default max history count (0 = unlimited)
const DEFAULT_MAX_HISTORY_COUNT: i64 = 0;

/// Clipboard content from the system
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ClipboardContent {
    Text(String),
    Html { html: String, text: Option<String> },
    Rtf { rtf: String, text: Option<String> },
    Image(Vec<u8>),
    Files(Vec<String>),
}

/// Handler for processing clipboard content
pub struct ClipboardHandler {
    repository: ClipboardRepository,
    settings_repo: SettingsRepository,
    images_path: PathBuf,
}

impl ClipboardHandler {
    pub fn new(db: &Database) -> Self {
        let images_path = get_images_path();
        // Ensure images directory exists
        std::fs::create_dir_all(&images_path).ok();
        
        Self {
            repository: ClipboardRepository::new(db),
            settings_repo: SettingsRepository::new(db),
            images_path,
        }
    }

    /// Get max content size from settings (in bytes)
    fn get_max_content_size(&self) -> usize {
        self.settings_repo
            .get("max_content_size_kb")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|kb| kb * 1024) // Convert KB to bytes
            .unwrap_or(DEFAULT_MAX_CONTENT_SIZE)
    }

    /// Get max history count from settings
    fn get_max_history_count(&self) -> i64 {
        self.settings_repo
            .get("max_history_count")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY_COUNT)
    }

    /// Process clipboard content and store if new
    pub fn process(&self, content: ClipboardContent) -> Result<Option<i64>, String> {
        // Get settings
        let max_content_size = self.get_max_content_size();
        
        // Check content size before processing
        let content_size = self.get_content_size(&content);
        if content_size > max_content_size {
            warn!(
                "Content size {} bytes exceeds max {} bytes, skipping",
                content_size, max_content_size
            );
            return Ok(None);
        }

        // Calculate content hash
        let hash = self.calculate_hash(&content);
        
        // Check if already exists
        if self.repository.exists_by_hash(&hash).map_err(|e| e.to_string())? {
            // Update access time and return existing id
            debug!("Content already exists, updating access time");
            return self.repository.touch_by_hash(&hash).map_err(|e| e.to_string());
        }

        // Create new item based on content type
        let item = match content {
            ClipboardContent::Text(text) => self.process_text(text, hash, max_content_size)?,
            ClipboardContent::Html { html, text } => self.process_html(html, text, hash, max_content_size)?,
            ClipboardContent::Rtf { rtf, text } => self.process_rtf(rtf, text, hash, max_content_size)?,
            ClipboardContent::Image(data) => self.process_image(data, hash)?,
            ClipboardContent::Files(files) => self.process_files(files, hash)?,
        };

        // Insert into database
        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!("Stored new clipboard item with id: {}", id);

        // Enforce max history count
        let max_history_count = self.get_max_history_count();
        if max_history_count > 0 {
            if let Err(e) = self.repository.enforce_max_count(max_history_count) {
                warn!("Failed to enforce max history count: {}", e);
            }
        }

        Ok(Some(id))
    }

    /// Get the size of clipboard content in bytes
    fn get_content_size(&self, content: &ClipboardContent) -> usize {
        match content {
            ClipboardContent::Text(text) => text.len(),
            ClipboardContent::Html { html, .. } => html.len(),
            ClipboardContent::Rtf { rtf, .. } => rtf.len(),
            ClipboardContent::Image(data) => data.len(),
            ClipboardContent::Files(files) => files.iter().map(|f| f.len()).sum(),
        }
    }

    /// Calculate hash of content
    fn calculate_hash(&self, content: &ClipboardContent) -> String {
        let mut hasher = Hasher::new();
        
        match content {
            ClipboardContent::Text(text) => {
                hasher.update(b"text:");
                hasher.update(text.as_bytes());
            }
            ClipboardContent::Html { html, .. } => {
                hasher.update(b"html:");
                hasher.update(html.as_bytes());
            }
            ClipboardContent::Rtf { rtf, .. } => {
                hasher.update(b"rtf:");
                hasher.update(rtf.as_bytes());
            }
            ClipboardContent::Image(data) => {
                hasher.update(b"image:");
                hasher.update(data);
            }
            ClipboardContent::Files(files) => {
                hasher.update(b"files:");
                for file in files {
                    hasher.update(file.as_bytes());
                    hasher.update(b"|");
                }
            }
        }

        hasher.finalize().to_hex().to_string()
    }

    /// Process text content
    fn process_text(&self, text: String, hash: String, max_size: usize) -> Result<NewClipboardItem, String> {
        let byte_size = text.len() as i64;
        let preview = Self::create_preview(&text);
        
        // Truncate if too long (safely at char boundary)
        let text_content = if text.len() > max_size {
            warn!("Text content truncated from {} to {} bytes", text.len(), max_size);
            text.char_indices()
                .take_while(|(i, _)| *i < max_size)
                .map(|(_, c)| c)
                .collect()
        } else {
            text
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Text,
            text_content: Some(text_content),
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: None,
            content_hash: hash,
            preview: Some(preview),
            byte_size,
        })
    }

    /// Process HTML content
    fn process_html(&self, html: String, text: Option<String>, hash: String, max_size: usize) -> Result<NewClipboardItem, String> {
        let byte_size = html.len() as i64;
        let preview = text.as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| Self::create_preview(&html));

        // Truncate HTML if too long
        let html_content = if html.len() > max_size {
            warn!("HTML content truncated from {} to {} bytes", html.len(), max_size);
            html.char_indices()
                .take_while(|(i, _)| *i < max_size)
                .map(|(_, c)| c)
                .collect()
        } else {
            html
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Html,
            text_content: text,
            html_content: Some(html_content),
            rtf_content: None,
            image_path: None,
            file_paths: None,
            content_hash: hash,
            preview: Some(preview),
            byte_size,
        })
    }

    /// Process RTF content
    fn process_rtf(&self, rtf: String, text: Option<String>, hash: String, max_size: usize) -> Result<NewClipboardItem, String> {
        let byte_size = rtf.len() as i64;
        let preview = text.as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| "[RTF Content]".to_string());

        // Truncate RTF if too long
        let rtf_content = if rtf.len() > max_size {
            warn!("RTF content truncated from {} to {} bytes", rtf.len(), max_size);
            rtf.char_indices()
                .take_while(|(i, _)| *i < max_size)
                .map(|(_, c)| c)
                .collect()
        } else {
            rtf
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Rtf,
            text_content: text,
            html_content: None,
            rtf_content: Some(rtf_content),
            image_path: None,
            file_paths: None,
            content_hash: hash,
            preview: Some(preview),
            byte_size,
        })
    }

    /// Process image content
    fn process_image(&self, data: Vec<u8>, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = data.len() as i64;
        
        // Generate unique filename
        let filename = format!("{}.png", &hash[..16]);
        let image_path = self.images_path.join(&filename);
        
        // Save image data directly (it's already PNG from monitor)
        let mut file = std::fs::File::create(&image_path)
            .map_err(|e| format!("Failed to create image file: {}", e))?;
        file.write_all(&data)
            .map_err(|e| format!("Failed to write image data: {}", e))?;
        debug!("Saved image to {:?}", image_path);

        Ok(NewClipboardItem {
            content_type: ContentType::Image,
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: Some(image_path.to_string_lossy().to_string()),
            file_paths: None,
            content_hash: hash,
            preview: Some("[Image]".to_string()),
            byte_size,
        })
    }

    /// Process file paths
    fn process_files(&self, files: Vec<String>, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = files.iter().map(|f| f.len()).sum::<usize>() as i64;
        let preview = if files.len() == 1 {
            files[0].clone()
        } else {
            format!("{} files", files.len())
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Files,
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: Some(files),
            content_hash: hash,
            preview: Some(preview),
            byte_size,
        })
    }

    /// Create preview text
    fn create_preview(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.len() <= MAX_PREVIEW_LENGTH {
            trimmed.to_string()
        } else {
            format!("{}...", &trimmed[..MAX_PREVIEW_LENGTH])
        }
    }
}
