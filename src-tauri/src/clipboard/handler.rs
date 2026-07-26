use super::file_clipboard::{self, FileCaptureData};
use super::source_app::{self, SourceAppInfo};
use super::{
    canonical_url_text, compute_semantic_hash, is_url, normalize_rtf_for_hash,
    semantic_hash_from_text,
};
use crate::database::{
    ClipboardRepository, ContentType, Database, NewClipboardItem, SettingsRepository,
};
use base64::Engine;
use blake3::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::file_preview_limits::{
    DEFAULT_MAX_IMAGE_SIZE_KB, is_too_large_for_preview, preview_limit_bytes,
};
const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;
const MAX_PREVIEW_LENGTH: usize = 200;
const DEFAULT_MAX_HISTORY_COUNT: i64 = 0;
const DEFAULT_AUTO_CLEANUP_DAYS: i64 = 30;

/// 通配符匹配（支持 * 和 ?，不区分大小写，O(n) 空间）
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let tlen = text.len();

    let mut prev = vec![false; tlen + 1];
    let mut curr = vec![false; tlen + 1];
    prev[0] = true;

    for &pc in &pattern {
        curr.fill(false);
        if pc == '*' {
            curr[0] = prev[0];
            for j in 0..tlen {
                curr[j + 1] = prev[j + 1] || curr[j];
            }
        } else {
            for (j, &tc) in text.iter().enumerate() {
                if pc == '?' || pc == tc {
                    curr[j + 1] = prev[j];
                }
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[tlen]
}

/// 检查来源应用是否匹配过滤规则
/// 支持通配符模式和普通子串匹配，匹配目标：应用名、进程名、进程路径
fn matches_app_filter(filter: &str, app_name: &str, exe_name: &str, exe_path: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return false;
    }

    if filter.contains('*') || filter.contains('?') {
        wildcard_match(filter, app_name)
            || wildcard_match(filter, exe_name)
            || wildcard_match(filter, exe_path)
    } else {
        let f = filter.to_lowercase();
        app_name.to_lowercase().contains(&f)
            || exe_name.to_lowercase().contains(&f)
            || exe_path.to_lowercase().contains(&f)
    }
}

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

/// 判断文件路径是否为常见图片格式
fn is_image_file(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tiff" | "tif")
    )
}

#[derive(Debug, Clone, Default)]
pub struct ImageCapture {
    pub temp_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
}

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Html {
        html: String,
        text: Option<String>,
        rtf: Option<String>,
    },
    Rtf {
        rtf: String,
        text: Option<String>,
    },
    ImageFile(ImageCapture),
    Files(FileCaptureData),
}
pub(crate) fn cleanup_capture_content(content: &ClipboardContent) {
    if let ClipboardContent::ImageFile(capture) = content
        && let Err(e) = std::fs::remove_file(&capture.temp_path)
    {
        debug!(
            "Failed to remove capture temp file {:?}: {}",
            capture.temp_path, e
        );
    }
}

impl Drop for ClipboardContent {
    fn drop(&mut self) {
        cleanup_capture_content(self);
    }
}

#[derive(Debug, Clone)]
struct ContentHashes {
    content_hash: String,
    semantic_hash: String,
}

/// 处理剪贴板内容所需的设置，批量读取避免多次数据库查询
struct ProcessSettings {
    max_content_size: usize,
    dedup_strategy: String,
    text_dedup_mode: String,
    max_history_count: i64,
    auto_cleanup_days: i64,
    max_image_size_kb: u64,
}

/// 剪贴板变更热路径所需的设置，单次批量查询
#[derive(Debug, Clone)]
pub struct ClipChangeSettings {
    app_filter_enabled: bool,
    app_filter_list: Option<String>,
    app_filter_mode: String,
    pub max_image_bytes: usize,
    monitor_types: Option<String>,
}

impl Default for ClipChangeSettings {
    fn default() -> Self {
        Self {
            app_filter_enabled: false,
            app_filter_list: None,
            app_filter_mode: "blacklist".to_string(),
            max_image_bytes: (DEFAULT_MAX_IMAGE_SIZE_KB * 1024) as usize,
            monitor_types: None,
        }
    }
}

impl ClipChangeSettings {
    /// 检查来源应用是否应被过滤（无 DB 查询）
    pub fn is_source_app_excluded(&self, source: &Option<SourceAppInfo>) -> bool {
        let Some(source) = source else {
            return false;
        };

        if !self.app_filter_enabled {
            return false;
        }

        let filter_list = match self.app_filter_list.as_deref() {
            Some(s) => s,
            None => return false,
        };

        let exe_name = std::path::Path::new(&source.exe_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let matches = filter_list.split(',').any(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return false;
            }
            matches_app_filter(entry, &source.app_name, exe_name, &source.exe_path)
        });

        match self.app_filter_mode.as_str() {
            "whitelist" => !matches,
            _ => matches,
        }
    }

    /// 检查内容类型是否被允许监听（无 DB 查询）
    pub fn is_content_type_allowed(&self, content: &ClipboardContent) -> bool {
        let allowed = match self.monitor_types.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => return true,
        };

        let content_type = match content {
            ClipboardContent::Text(text) if is_url(text) => "url",
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Html { .. } => "html",
            ClipboardContent::Rtf { .. } => "rtf",
            ClipboardContent::ImageFile(_) => "image",
            ClipboardContent::Files(_) => "files",
        };

        if content_type == "url" && !allowed.split(',').any(|t| t.trim() == "url") {
            return false;
        }

        allowed.split(',').any(|t| t.trim() == content_type)
    }
}

pub struct ClipboardHandler {
    repository: ClipboardRepository,
    settings_repo: SettingsRepository,
    images_path: PathBuf,
    icons_path: PathBuf,
    /// 内存级去重：最近一次成功处理的内容哈希，防止快速连续事件绕过 DB dedup
    last_content_hash: parking_lot::Mutex<String>,
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
            last_content_hash: parking_lot::Mutex::new(String::new()),
        }
    }

    /// 批量读取处理所需的全部设置，单次数据库查询
    fn get_process_settings(&self) -> ProcessSettings {
        let keys = [
            "max_content_size_kb",
            "dedup_strategy",
            "text_dedup_mode",
            "max_history_count",
            "auto_cleanup_days",
            "max_image_size_kb",
        ];
        let batch = self.settings_repo.get_batch(&keys);

        let max_content_size = batch
            .get("max_content_size_kb")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(DEFAULT_MAX_CONTENT_SIZE, |kb| kb * 1024);

        let dedup_strategy = batch
            .get("dedup_strategy")
            .and_then(|v| v.as_deref())
            .unwrap_or("move_to_top")
            .to_string();

        let text_dedup_mode = batch
            .get("text_dedup_mode")
            .and_then(|v| v.as_deref())
            .unwrap_or("semantic")
            .to_string();

        let max_history_count = batch
            .get("max_history_count")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY_COUNT);

        let auto_cleanup_days = batch
            .get("auto_cleanup_days")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_AUTO_CLEANUP_DAYS);

        let max_image_size_kb = batch
            .get("max_image_size_kb")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_MAX_IMAGE_SIZE_KB);

        ProcessSettings {
            max_content_size,
            dedup_strategy,
            text_dedup_mode,
            max_history_count,
            auto_cleanup_days,
            max_image_size_kb,
        }
    }

    /// 批量读取剪贴板变更热路径所需的全部设置，单次数据库查询
    pub(crate) fn get_clip_change_settings(&self) -> ClipChangeSettings {
        let keys = [
            "app_filter_enabled",
            "app_filter_list",
            "app_filter_mode",
            "max_image_size_kb",
            "monitor_types",
        ];
        let batch = self.settings_repo.get_batch(&keys);

        let app_filter_enabled = batch
            .get("app_filter_enabled")
            .and_then(|v| v.as_deref())
            .is_some_and(|v| v == "true");

        let app_filter_list = batch
            .get("app_filter_list")
            .and_then(|v| v.as_deref())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let app_filter_mode = batch
            .get("app_filter_mode")
            .and_then(|v| v.as_deref())
            .unwrap_or("blacklist")
            .to_string();

        let max_image_bytes = batch
            .get("max_image_size_kb")
            .and_then(|v| v.as_deref())
            .and_then(|s| s.parse::<usize>().ok())
            .map_or((DEFAULT_MAX_IMAGE_SIZE_KB * 1024) as usize, |kb| {
                kb.saturating_mul(1024)
            });

        let monitor_types = batch
            .get("monitor_types")
            .and_then(|v| v.as_deref())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        ClipChangeSettings {
            app_filter_enabled,
            app_filter_list,
            app_filter_mode,
            max_image_bytes,
            monitor_types,
        }
    }

    /// 处理剪贴板内容，去重后存入数据库
    pub fn process(
        &self,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
        group_id: Option<i64>,
    ) -> Result<Option<i64>, String> {
        // 批量读取所有设置，单次数据库查询替代 5-6 次独立查询
        let settings = self.get_process_settings();
        let max_content_size = settings.max_content_size;

        // max_content_size 仅限制文本类内容
        if max_content_size > 0 {
            let is_text_content = Self::is_text_like_content(&content);
            if is_text_content {
                let content_size = self.get_content_size(&content);
                if content_size > max_content_size {
                    warn!(
                        "Content size {} bytes exceeds max {} bytes, skipping",
                        content_size, max_content_size
                    );
                    return Ok(None);
                }
            }
        }

        let hashes = self.calculate_hashes(&content)?;
        let dedup = &settings.dedup_strategy;
        let text_like = Self::is_text_like_content(&content);
        let text_dedup_mode = &settings.text_dedup_mode;
        let text_use_strict = text_like && text_dedup_mode == "strict";

        // 内存级去重：防止快速连续事件（如 Zen 浏览器 1ms 内两次事件）绕过 DB dedup
        // 仅在 dedup 策略不是 always_new 时生效
        if dedup != "always_new" {
            let mut last = self.last_content_hash.lock();
            if *last == hashes.content_hash {
                debug!("Content hash matches last processed, skipping (memory dedup)");
                return Ok(None);
            }
            last.clone_from(&hashes.content_hash);
        }

        if dedup != "always_new"
            && if text_like {
                if text_use_strict {
                    self.repository
                        .exists_by_hash(&hashes.content_hash, group_id)
                } else {
                    self.repository
                        .exists_by_semantic_hash(&hashes.semantic_hash, group_id)
                }
            } else {
                self.repository
                    .exists_by_hash(&hashes.content_hash, group_id)
            }
            .map_err(|e| e.to_string())?
        {
            if dedup.as_str() == "ignore" {
                debug!("Content already exists, ignoring (dedup=ignore)");
                return Ok(None);
            } else {
                // move_to_top: 更新访问时间并置顶；HTML/RTF 同时刷新富文本字段
                debug!("Content already exists, updating access time (dedup=move_to_top)");
                let id = if text_like {
                    if text_use_strict {
                        self.repository
                            .touch_by_hash(&hashes.content_hash, group_id)
                    } else {
                        self.repository
                            .touch_by_semantic_hash(&hashes.semantic_hash, group_id)
                    }
                } else {
                    self.repository
                        .touch_by_hash(&hashes.content_hash, group_id)
                }
                .map_err(|e| e.to_string())?;

                if let (Some(id), true) = (
                    id,
                    matches!(
                        content,
                        ClipboardContent::Html { .. } | ClipboardContent::Rtf { .. }
                    ),
                ) {
                    let refreshed = match &content {
                        ClipboardContent::Html { html, text, rtf } => self.process_html(
                            html.clone(),
                            text.clone(),
                            rtf.clone(),
                            &hashes,
                            max_content_size,
                        )?,
                        ClipboardContent::Rtf { rtf, text } => {
                            self.process_rtf(rtf.clone(), text.clone(), &hashes, max_content_size)?
                        }
                        _ => unreachable!(),
                    };
                    self.repository
                        .refresh_rich_fields(id, &refreshed)
                        .map_err(|e| e.to_string())?;
                }

                return Ok(id);
            }
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
        // 安全地从 ClipboardContent（impl Drop）中取出内部值
        // 通过 ManuallyDrop 阻止 Drop 运行，process_image_file 已自行处理临时文件
        let mut item = {
            let mut content = std::mem::ManuallyDrop::new(content);
            // SAFETY: 逐字段 take 所有权，ManuallyDrop 阻止外层 Drop
            match &mut *content {
                ClipboardContent::Text(text) => {
                    let text = std::mem::take(text);
                    self.process_text(text, &hashes, max_content_size)?
                }
                ClipboardContent::Html { html, text, rtf } => {
                    let html = std::mem::take(html);
                    let text = std::mem::take(text);
                    let rtf = std::mem::take(rtf);
                    self.process_html(html, text, rtf, &hashes, max_content_size)?
                }
                ClipboardContent::Rtf { rtf, text } => {
                    let rtf = std::mem::take(rtf);
                    let text = std::mem::take(text);
                    self.process_rtf(rtf, text, &hashes, max_content_size)?
                }
                ClipboardContent::ImageFile(capture) => {
                    let capture = std::mem::take(capture);
                    self.process_image_file(capture, &hashes)?
                }
                ClipboardContent::Files(files) => {
                    let files = std::mem::take(files);
                    // staging 使用 file_clipboard 内置 50MB 上限，与文本长度限制无关
                    self.process_files(files, &hashes, 0, settings.max_image_size_kb)?
                }
            }
        };

        item.source_app_name = source_app_name;
        item.source_app_icon = source_app_icon;
        item.group_id = group_id;

        let log_type = format!("{:?}", item.content_type);
        let log_size = item.byte_size;
        let log_source = item
            .source_app_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!(
            "Stored clipboard item: id={}, type={}, size={} bytes, source={}",
            id, log_type, log_size, log_source
        );

        // 执行最大历史数限制，清理旧图片
        if settings.max_history_count > 0 {
            match self
                .repository
                .enforce_max_count(settings.max_history_count, group_id)
            {
                Ok((deleted, image_paths, file_payloads)) => {
                    super::cleanup_deleted_assets(&image_paths, &file_payloads);
                    if deleted > 0 {
                        debug!("Enforced max count: removed {} old items", deleted);
                    }
                }
                Err(e) => warn!("Failed to enforce max history count: {}", e),
            }
        }

        // 自动清理超过指定天数的旧记录
        if settings.auto_cleanup_days > 0 {
            match self
                .repository
                .delete_older_than(settings.auto_cleanup_days, group_id)
            {
                Ok((deleted, image_paths, file_payloads)) => {
                    super::cleanup_deleted_assets(&image_paths, &file_payloads);
                    if deleted > 0 {
                        info!(
                            "Auto-cleanup: removed {} items older than {} days",
                            deleted, settings.auto_cleanup_days
                        );
                    }
                }
                Err(e) => warn!("Failed to auto-cleanup old items: {}", e),
            }
        }

        Ok(Some(id))
    }

    fn get_content_size(&self, content: &ClipboardContent) -> usize {
        match content {
            ClipboardContent::Text(text) => text.len(),
            ClipboardContent::Html { html, .. } => html.len(),
            ClipboardContent::Rtf { rtf, .. } => rtf.len(),
            ClipboardContent::ImageFile(capture) => capture.byte_size,
            ClipboardContent::Files(files) => {
                files.paths.iter().map(std::string::String::len).sum()
            }
        }
    }

    fn is_text_like_content(content: &ClipboardContent) -> bool {
        matches!(
            content,
            ClipboardContent::Text(_)
                | ClipboardContent::Html { .. }
                | ClipboardContent::Rtf { .. }
        )
    }

    fn calculate_hashes(&self, content: &ClipboardContent) -> Result<ContentHashes, String> {
        let content_hash = self.calculate_hash(content)?;
        let semantic_hash = match content {
            ClipboardContent::Text(text) => {
                if is_url(text) {
                    content_hash.clone()
                } else {
                    semantic_hash_from_text(text).unwrap_or_else(|| content_hash.clone())
                }
            }
            ClipboardContent::Html { text, .. } => {
                compute_semantic_hash("html", text.as_deref(), &content_hash)
            }
            ClipboardContent::Rtf { text, .. } => {
                compute_semantic_hash("rtf", text.as_deref(), &content_hash)
            }
            ClipboardContent::ImageFile(_) => content_hash.clone(),
            ClipboardContent::Files(_) => content_hash.clone(),
        };

        Ok(ContentHashes {
            content_hash,
            semantic_hash,
        })
    }

    fn calculate_hash(&self, content: &ClipboardContent) -> Result<String, String> {
        let mut hasher = Hasher::new();

        match content {
            ClipboardContent::Text(text) => {
                if let Some(canonical) = canonical_url_text(text) {
                    hasher.update(b"url:");
                    hasher.update(canonical.as_bytes());
                } else {
                    hasher.update(b"text:");
                    hasher.update(text.as_bytes());
                }
            }
            ClipboardContent::Html { html, .. } => {
                hasher.update(b"html:");
                hasher.update(html.as_bytes());
            }
            ClipboardContent::Rtf { rtf, .. } => {
                hasher.update(b"rtf:");
                // Decode b64, strip volatile fields (rsid, datastore…), hash cleaned bytes.
                if let Some(b64) = rtf.strip_prefix("b64:") {
                    if let Ok(raw) =
                        base64::engine::general_purpose::STANDARD.decode(b64.as_bytes())
                    {
                        let cleaned = normalize_rtf_for_hash(&raw);
                        hasher.update(&cleaned);
                    } else {
                        hasher.update(rtf.as_bytes());
                    }
                } else {
                    hasher.update(rtf.as_bytes());
                }
            }
            ClipboardContent::ImageFile(capture) => {
                hasher.update(b"image:");
                hash_file_into_hasher(&mut hasher, &capture.temp_path)?;
            }
            ClipboardContent::Files(files) => {
                hasher.update(b"files:");
                for file in &files.paths {
                    hasher.update(file.as_bytes());
                    hasher.update(b"|");
                }
                if files.paths.is_empty() {
                    for name in file_clipboard::descriptor_file_names(&files.extra_formats) {
                        hasher.update(name.as_bytes());
                        hasher.update(b"|");
                    }
                }
                if let Some(ref raw) = files.hdrop_raw {
                    hasher.update(b"hdrop:");
                    hasher.update(raw);
                }
            }
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    fn process_text(
        &self,
        text: String,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let is_url = canonical_url_text(&text).is_some();
        let text = if is_url {
            text.trim().to_string()
        } else {
            text
        };
        let char_count = Some(text.chars().count() as i64);
        let preview = Self::create_preview(&text);
        let content_type = if is_url {
            ContentType::Url
        } else {
            ContentType::Text
        };
        let text_content = truncate_content(text, max_size, "Text");
        let byte_size = text_content.len() as i64;

        Ok(NewClipboardItem {
            content_type,
            text_content: Some(text_content),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    fn process_html(
        &self,
        html: String,
        text: Option<String>,
        rtf: Option<String>,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let preview = text
            .as_ref()
            .map_or_else(|| Self::create_preview(&html), |t| Self::create_preview(t));
        let html_content = truncate_content(html, max_size, "HTML");
        let byte_size = html_content.len() as i64;
        let rtf_content = rtf.map(|r| super::rtf_storage::truncate_rtf_storage(r, max_size));

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

        Ok(NewClipboardItem {
            content_type: ContentType::Html,
            text_content: text,
            html_content: Some(html_content),
            rtf_content,
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    fn process_rtf(
        &self,
        rtf: String,
        text: Option<String>,
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let preview = text
            .as_ref()
            .map_or_else(|| "[RTF Content]".to_string(), |t| Self::create_preview(t));
        let rtf_content = super::rtf_storage::truncate_rtf_storage(rtf, max_size);
        let byte_size = rtf_content.len() as i64;

        let char_count = text.as_ref().map(|t| t.chars().count() as i64);

        Ok(NewClipboardItem {
            content_type: ContentType::Rtf,
            text_content: text,
            rtf_content: Some(rtf_content),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    /// 处理图片：将 watcher 写入的临时 PNG rename 到 hash 命名路径
    fn process_image_file(
        &self,
        capture: ImageCapture,
        hashes: &ContentHashes,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = capture.byte_size as i64;
        let image_width = i64::from(capture.width);
        let image_height = i64::from(capture.height);

        let filename = format!("{}.png", &hashes.content_hash[..32]);
        let image_path = self.images_path.join(&filename);
        let image_path_str = image_path.to_string_lossy().to_string();

        debug!(
            "Processing image: {}x{}, {} bytes, hash={}",
            image_width,
            image_height,
            byte_size,
            &hashes.content_hash[..32]
        );

        if image_path.exists() {
            let _ = std::fs::remove_file(&capture.temp_path);
        } else if let Err(e) = std::fs::rename(&capture.temp_path, &image_path) {
            let _ = std::fs::remove_file(&capture.temp_path);
            return Err(format!("Failed to save image: {e}"));
        }
        debug!("Saved image to {:?}", image_path);

        Ok(NewClipboardItem {
            content_type: ContentType::Image,
            image_path: Some(image_path_str),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some("[图片]".to_string()),
            byte_size,
            image_width: Some(image_width),
            image_height: Some(image_height),
            ..Default::default()
        })
    }

    fn process_files(
        &self,
        capture: FileCaptureData,
        hashes: &ContentHashes,
        max_stage_bytes: usize,
        max_image_size_kb: u64,
    ) -> Result<NewClipboardItem, String> {
        debug!("Processing {} file(s)", capture.paths.len());

        let stored_paths = file_clipboard::effective_file_paths(&capture);

        let byte_size: i64 = capture
            .paths
            .iter()
            .filter_map(|f| {
                let path = Path::new(f);
                if path.is_file() {
                    std::fs::metadata(path).ok().map(|m| m.len() as i64)
                } else {
                    None
                }
            })
            .sum();

        if capture.paths.len() == 1
            && is_image_file(&capture.paths[0])
            && is_too_large_for_preview(&capture.paths[0], byte_size, max_image_size_kb, false)
        {
            let limit = preview_limit_bytes(&capture.paths[0], max_image_size_kb);
            warn!(
                "Single image file {} ({} bytes) exceeds preview limit {} bytes, storing as file entry without image preview",
                capture.paths[0], byte_size, limit
            );
        }

        let preview =
            file_clipboard::file_entry_preview(&capture.paths, &capture.extra_formats);

        let staged_dir = self
            .images_path
            .parent()
            .unwrap_or(&self.images_path)
            .join("staged")
            .join(&hashes.content_hash[..8.min(hashes.content_hash.len())]);
        let payload = file_clipboard::build_payload(&capture, &staged_dir, max_stage_bytes as u64);
        let file_payload = Some(file_clipboard::encode_payload(&payload));

        Ok(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(stored_paths),
            file_payload,
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            ..Default::default()
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

fn hash_file_into_hasher(hasher: &mut Hasher, path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open image file for hash: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read image file for hash: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(())
}

/// 清理 images/captures 目录中的残留临时文件
pub fn cleanup_stale_capture_files(capture_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(capture_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|ext| ext == "tmp" || ext == "dib")
        {
            std::fs::remove_file(path).ok();
        }
    }
}

/// 清理旧版遗留的 `.dib` 伴侣文件（现已不再存储）
pub fn cleanup_legacy_dib_companions(images_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(images_dir) else {
        return;
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "dib") && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        info!("Removed {} legacy .dib companion file(s)", removed);
    }
}
