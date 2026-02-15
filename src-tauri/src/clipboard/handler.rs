use super::source_app::{self, SourceAppInfo};
use crate::database::{
    ClipboardRepository, ContentType, Database, NewClipboardItem, SettingsRepository,
};
use blake3::Hasher;
use image::ImageReader;
use std::path::PathBuf;
use tracing::{debug, info, warn};

const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;
const MAX_PREVIEW_LENGTH: usize = 200;
const DEFAULT_MAX_HISTORY_COUNT: i64 = 0;

/// 按字符边界截断超长内容
fn truncate_content(content: String, max_size: usize, content_type: &str) -> String {
    if max_size > 0 && content.len() > max_size {
        warn!(
            "{} content truncated from {} to {} bytes",
            content_type,
            content.len(),
            max_size
        );
        content
            .char_indices()
            .take_while(|(i, _)| *i < max_size)
            .map(|(_, c)| c)
            .collect()
    } else {
        content
    }
}

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    #[allow(dead_code)] // 预留: 监听器尚未捕获
    Html {
        html: String,
        text: Option<String>,
    },
    #[allow(dead_code)] // 预留: 监听器尚未捕获
    Rtf {
        rtf: String,
        text: Option<String>,
    },
    Image(Vec<u8>),
    Files(Vec<String>),
}

pub struct ClipboardHandler {
    repository: ClipboardRepository,
    settings_repo: SettingsRepository,
    images_path: PathBuf,
    icons_path: PathBuf,
}

impl ClipboardHandler {
    pub fn new(db: &Database, images_path: PathBuf) -> Self {
        std::fs::create_dir_all(&images_path).ok();

        // 图标目录与图片目录同级
        let icons_path = images_path.parent().unwrap_or(&images_path).join("icons");
        std::fs::create_dir_all(&icons_path).ok();

        Self {
            repository: ClipboardRepository::new(db),
            settings_repo: SettingsRepository::new(db),
            images_path,
            icons_path,
        }
    }

    fn get_max_content_size(&self) -> usize {
        self.settings_repo
            .get("max_content_size_kb")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(DEFAULT_MAX_CONTENT_SIZE)
    }

    fn get_max_history_count(&self) -> i64 {
        self.settings_repo
            .get("max_history_count")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY_COUNT)
    }

    /// 处理剪贴板内容，去重后存入数据库
    pub fn process(
        &self,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
    ) -> Result<Option<i64>, String> {
        let max_content_size = self.get_max_content_size();

        if max_content_size > 0 {
            let content_size = self.get_content_size(&content);
            if content_size > max_content_size {
                warn!(
                    "Content size {} bytes exceeds max {} bytes, skipping",
                    content_size, max_content_size
                );
                return Ok(None);
            }
        }

        let hash = self.calculate_hash(&content);

        if self
            .repository
            .exists_by_hash(&hash)
            .map_err(|e| e.to_string())?
        {
            debug!("Content already exists, updating access time");
            return self
                .repository
                .touch_by_hash(&hash)
                .map_err(|e| e.to_string());
        }

        let (source_app_name, source_app_icon) = match source {
            Some(ref info) => {
                let icon_path = source_app::extract_and_cache_icon(
                    &info.exe_path,
                    &self.icons_path,
                    &info.icon_cache_key,
                );
                (Some(info.app_name.clone()), icon_path)
            }
            None => (None, None),
        };

        let mut item = match content {
            ClipboardContent::Text(text) => self.process_text(text, hash, max_content_size)?,
            ClipboardContent::Html { html, text } => {
                self.process_html(html, text, hash, max_content_size)?
            }
            ClipboardContent::Rtf { rtf, text } => {
                self.process_rtf(rtf, text, hash, max_content_size)?
            }
            ClipboardContent::Image(data) => self.process_image(data, hash)?,
            ClipboardContent::Files(files) => self.process_files(files, hash)?,
        };

        item.source_app_name = source_app_name;
        item.source_app_icon = source_app_icon;

        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!("Stored new clipboard item with id: {}", id);

        // 执行最大历史数限制，清理旧图片
        let max_history_count = self.get_max_history_count();
        if max_history_count > 0 {
            match self.repository.enforce_max_count(max_history_count) {
                Ok((deleted, image_paths)) => {
                    for path in image_paths {
                        if let Err(e) = std::fs::remove_file(&path) {
                            debug!("Failed to delete old image file {}: {}", path, e);
                        } else {
                            debug!("Deleted old image file: {}", path);
                        }
                    }
                    if deleted > 0 {
                        debug!("Enforced max count: removed {} old items", deleted);
                    }
                }
                Err(e) => {
                    warn!("Failed to enforce max history count: {}", e);
                }
            }
        }

        Ok(Some(id))
    }

    fn get_content_size(&self, content: &ClipboardContent) -> usize {
        match content {
            ClipboardContent::Text(text) => text.len(),
            ClipboardContent::Html { html, .. } => html.len(),
            ClipboardContent::Rtf { rtf, .. } => rtf.len(),
            ClipboardContent::Image(data) => data.len(),
            ClipboardContent::Files(files) => files.iter().map(|f| f.len()).sum(),
        }
    }

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

    fn process_text(
        &self,
        text: String,
        hash: String,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = text.len() as i64;
        let char_count = Some(text.chars().count() as i64);
        let preview = Self::create_preview(&text);
        let text_content = truncate_content(text, max_size, "Text");

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
            image_width: None,
            image_height: None,
            char_count,
            source_app_name: None,
            source_app_icon: None,
        })
    }

    fn process_html(
        &self,
        html: String,
        text: Option<String>,
        hash: String,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = html.len() as i64;
        let preview = text
            .as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| Self::create_preview(&html));
        let html_content = truncate_content(html, max_size, "HTML");

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

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
            image_width: None,
            image_height: None,
            char_count,
            source_app_name: None,
            source_app_icon: None,
        })
    }

    fn process_rtf(
        &self,
        rtf: String,
        text: Option<String>,
        hash: String,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = rtf.len() as i64;
        let preview = text
            .as_ref()
            .map(|t| Self::create_preview(t))
            .unwrap_or_else(|| "[RTF Content]".to_string());
        let rtf_content = truncate_content(rtf, max_size, "RTF");

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

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
            image_width: None,
            image_height: None,
            char_count,
            source_app_name: None,
            source_app_icon: None,
        })
    }

    /// Process image content
    /// Saves image to disk and extracts metadata (width, height)
    /// Uses background thread for file I/O to avoid blocking the monitor
    fn process_image(&self, data: Vec<u8>, hash: String) -> Result<NewClipboardItem, String> {
        let byte_size = data.len() as i64;

        let filename = format!("{}.png", &hash[..16]);
        let image_path = self.images_path.join(&filename);
        let image_path_str = image_path.to_string_lossy().to_string();

        let (image_width, image_height) = self.extract_image_dimensions(&data)?;

        // Save image file synchronously to ensure it exists before DB insert
        // (async write caused race: frontend could query the item before file was written)
        if let Err(e) = std::fs::write(&image_path, &data) {
            return Err(format!("Failed to save image: {}", e));
        }
        debug!("Saved image to {:?}", image_path);

        Ok(NewClipboardItem {
            content_type: ContentType::Image,
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: Some(image_path_str),
            file_paths: None,
            content_hash: hash,
            preview: Some("[图片]".to_string()),
            byte_size,
            image_width: Some(image_width),
            image_height: Some(image_height),
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
        })
    }

    fn extract_image_dimensions(&self, data: &[u8]) -> Result<(i64, i64), String> {
        let (w, h) = ImageReader::new(std::io::Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| format!("Failed to guess image format: {}", e))?
            .into_dimensions()
            .map_err(|e| format!("Failed to read image dimensions: {}", e))?;

        Ok((w as i64, h as i64))
    }

    fn process_files(&self, files: Vec<String>, hash: String) -> Result<NewClipboardItem, String> {
        use std::path::Path;

        // Calculate file sizes (only for regular files, skip directories)
        // Directory size calculation is expensive and low value
        let byte_size: i64 = files
            .iter()
            .filter_map(|f| {
                let path = Path::new(f);
                if path.is_file() {
                    std::fs::metadata(path).ok().map(|m| m.len() as i64)
                } else {
                    None // 跳过目录
                }
            })
            .sum();

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
            image_width: None,
            image_height: None,
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
        })
    }

    fn create_preview(text: &str) -> String {
        let trimmed = text.trim();
        if let Some((idx, _)) = trimmed.char_indices().nth(MAX_PREVIEW_LENGTH) {
            format!("{}...", &trimmed[..idx])
        } else {
            trimmed.to_string()
        }
    }
}
