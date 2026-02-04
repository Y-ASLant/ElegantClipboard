use crate::database::{
    get_images_path, ClipboardRepository, ContentType, Database, NewClipboardItem,
};
use blake3::Hasher;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Maximum text content length to store (1MB)
const MAX_TEXT_LENGTH: usize = 1_048_576;
/// Maximum preview length
const MAX_PREVIEW_LENGTH: usize = 200;

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
    images_path: PathBuf,
}

impl ClipboardHandler {
    pub fn new(db: &Database) -> Self {
        let images_path = get_images_path();
        // Ensure images directory exists
        std::fs::create_dir_all(&images_path).ok();
        
        Self {
            repository: ClipboardRepository::new(db),
            images_path,
        }
    }

    /// Process clipboard content and store if new
    pub fn process(&self, content: ClipboardContent) -> Result<Option<i64>, String> {
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
            ClipboardContent::Text(text) => self.process_text(text, hash)?,
            ClipboardContent::Html { html, text } => self.process_html(html, text, hash)?,
            ClipboardContent::Rtf { rtf, text } => self.process_rtf(rtf, text, hash)?,
            ClipboardContent::Image(data) => self.process_image(data, hash)?,
            ClipboardContent::Files(files) => self.process_files(files, hash)?,
        };

        // Insert into database
        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!("Stored new clipboard item with id: {}", id);
        Ok(Some(id))
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
    fn process_text(&self, text: String, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = text.len() as i64;
        let preview = Self::create_preview(&text);
        
        // Truncate if too long (safely at char boundary)
        let text_content = if text.len() > MAX_TEXT_LENGTH {
            warn!("Text content truncated from {} to {} bytes", text.len(), MAX_TEXT_LENGTH);
            text.char_indices()
                .take_while(|(i, _)| *i < MAX_TEXT_LENGTH)
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
    fn process_html(&self, html: String, text: Option<String>, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = html.len() as i64;
        let preview = text.as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| Self::create_preview(&html));

        Ok(NewClipboardItem {
            content_type: ContentType::Html,
            text_content: text,
            html_content: Some(html),
            rtf_content: None,
            image_path: None,
            file_paths: None,
            content_hash: hash,
            preview: Some(preview),
            byte_size,
        })
    }

    /// Process RTF content
    fn process_rtf(&self, rtf: String, text: Option<String>, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = rtf.len() as i64;
        let preview = text.as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| "[RTF Content]".to_string());

        Ok(NewClipboardItem {
            content_type: ContentType::Rtf,
            text_content: text,
            html_content: None,
            rtf_content: Some(rtf),
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
