use super::source_app::{self, SourceAppInfo};
use super::{compute_semantic_hash, semantic_hash_from_text};
use crate::database::{
    ClipboardRepository, ContentType, Database, NewClipboardItem, SettingsRepository,
};
use blake3::Hasher;
use image::ImageReader;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

const DEFAULT_MAX_CONTENT_SIZE: usize = 1_048_576;
const MAX_PREVIEW_LENGTH: usize = 200;
const DEFAULT_MAX_HISTORY_COUNT: i64 = 0;
const DEFAULT_AUTO_CLEANUP_DAYS: i64 = 30;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpeg", "mpg",
];

pub(super) fn is_video_files(files: &[String]) -> bool {
    !files.is_empty()
        && files.iter().all(|f| {
            let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            VIDEO_EXTENSIONS.contains(&ext.as_str())
        })
}

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
/// 参数 targets_lower 为预计算的小写 (app_name, exe_name, exe_path)，避免每条规则重复转换
fn matches_app_filter(filter: &str, targets_lower: &[&str; 3]) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return false;
    }

    if filter.contains('*') || filter.contains('?') {
        // 通配符模式：filter 在 wildcard_match 内部会 to_lowercase
        targets_lower.iter().any(|t| wildcard_match(filter, t))
    } else {
        // 普通子串匹配：filter 只需 to_lowercase 一次
        let f = filter.to_lowercase();
        targets_lower.iter().any(|t| t.contains(&f))
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

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Html {
        html: String,
        text: Option<String>,
    },
    Rtf {
        rtf: String,
        text: Option<String>,
    },
    Image(Vec<u8>),
    Files(Vec<String>),
    Video(Vec<String>),
}

#[derive(Debug, Clone)]
struct ContentHashes {
    content_hash: String,
    semantic_hash: String,
}

/// 缓存的内容过滤正则（避免每次剪贴板事件都重新编译）
struct CachedContentFilter {
    /// 原始规则文本（用于判断是否需要重新编译）
    rules_source: String,
    /// 编译后的 RegexSet（所有模式合法时使用）
    regex_set: Option<regex::RegexSet>,
    /// 逐条编译的正则（RegexSet 编译失败时的回退）
    individual_regexes: Vec<(String, regex::Regex)>,
}

impl CachedContentFilter {
    fn new() -> Self {
        Self {
            rules_source: String::new(),
            regex_set: None,
            individual_regexes: Vec::new(),
        }
    }

    /// 若规则文本变化则重新编译，返回是否有有效规则
    fn update_if_changed(&mut self, rules: &str) -> bool {
        if rules == self.rules_source {
            return self.regex_set.is_some() || !self.individual_regexes.is_empty();
        }

        self.rules_source = rules.to_string();
        self.regex_set = None;
        self.individual_regexes.clear();

        let patterns: Vec<&str> = rules
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if patterns.is_empty() {
            return false;
        }

        match regex::RegexSet::new(&patterns) {
            Ok(set) => {
                self.regex_set = Some(set);
            }
            Err(_) => {
                // RegexSet 要求所有模式合法；回退逐条编译
                for pattern in &patterns {
                    match regex::Regex::new(pattern) {
                        Ok(re) => {
                            self.individual_regexes.push((pattern.to_string(), re));
                        }
                        Err(e) => {
                            warn!("无效的内容过滤正则 {:?}: {}", pattern, e);
                        }
                    }
                }
            }
        }

        self.regex_set.is_some() || !self.individual_regexes.is_empty()
    }

    /// 检查文本是否匹配任一规则
    fn is_match(&self, text: &str) -> Option<&str> {
        if let Some(ref set) = self.regex_set {
            if let Some(_idx) = set.matches(text).iter().next() {
                // 无法从 RegexSet 获取具体模式文本，返回占位
                return Some("<regex_set_match>");
            }
        } else {
            for (pattern, re) in &self.individual_regexes {
                if re.is_match(text) {
                    return Some(pattern.as_str());
                }
            }
        }
        None
    }
}

/// 缓存的处理设置（避免每次剪贴板事件都查询数据库）
struct CachedProcessSettings {
    /// 设置值缓存（Arc 包装避免每次返回时克隆 HashMap）
    values: Arc<HashMap<String, String>>,
    /// 缓存版本号（设置变更时递增以失效）
    version: u64,
}

impl CachedProcessSettings {
    fn new() -> Self {
        Self {
            values: Arc::new(HashMap::new()),
            version: 0,
        }
    }
}

/// 全局设置版本号，前端/后端修改设置时递增以通知缓存失效
static SETTINGS_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// 外部调用：通知设置已变更，使缓存失效
pub fn invalidate_settings_cache() {
    SETTINGS_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub struct ClipboardHandler {
    repository: ClipboardRepository,
    settings_repo: SettingsRepository,
    images_path: PathBuf,
    icons_path: PathBuf,
    /// 缓存的内容过滤正则
    content_filter_cache: Mutex<CachedContentFilter>,
    /// 缓存的处理设置
    settings_cache: Mutex<CachedProcessSettings>,
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
            content_filter_cache: Mutex::new(CachedContentFilter::new()),
            settings_cache: Mutex::new(CachedProcessSettings::new()),
        }
    }

    // 以下设置读取方法已被 process() 中的 get_multiple 批量读取取代，不再单独调用

    /// 检查内容类型是否被允许监听（使用缓存设置）
    pub fn is_content_type_allowed_cached(&self, content: &ClipboardContent, settings: &HashMap<String, String>) -> bool {
        let allowed = settings.get("monitor_types");

        // 无设置或空字符串 → 全部允许
        let allowed = match allowed {
            Some(s) if !s.is_empty() => s,
            _ => return true,
        };

        let content_type = match content {
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Html { .. } => "html",
            ClipboardContent::Rtf { .. } => "rtf",
            ClipboardContent::Image(_) => "image",
            ClipboardContent::Files(_) => "files",
            ClipboardContent::Video(_) => "video",
        };

        allowed.split(',').any(|t| t.trim() == content_type)
    }

    /// 检查来源应用是否应被过滤（使用缓存设置，避免数据库查询）
    /// 设置项：
    ///   - `app_filter_enabled`: "true"/"false"（默认 false）
    ///   - `app_filter_mode`: "blacklist"（默认）/ "whitelist"
    ///   - `app_filter_list`: 逗号分隔的规则列表，支持通配符 * 和 ?
    ///
    /// 黑名单模式：匹配则排除；白名单模式：不匹配则排除
    pub fn is_source_app_excluded_cached(&self, source: &Option<super::source_app::SourceAppInfo>, settings: &HashMap<String, String>) -> bool {
        let source = match source {
            Some(s) => s,
            None => return false,
        };

        let enabled = settings.get("app_filter_enabled")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !enabled {
            return false;
        }

        let filter_list = match settings.get("app_filter_list") {
            Some(s) if !s.is_empty() => s,
            _ => return false,
        };

        let mode = settings.get("app_filter_mode")
            .map(|s| s.as_str())
            .unwrap_or("blacklist");

        let exe_name = std::path::Path::new(&source.exe_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let app_lower = source.app_name.to_lowercase();
        let exe_lower = exe_name.to_lowercase();
        let path_lower = source.exe_path.to_lowercase();
        let targets_lower = [app_lower.as_str(), exe_lower.as_str(), path_lower.as_str()];

        let matches = filter_list.split(',').any(|entry| {
            let entry = entry.trim();
            if entry.is_empty() { return false; }
            matches_app_filter(entry, &targets_lower)
        });

        match mode {
            "whitelist" => !matches,
            _ => matches,
        }
    }

    /// 检查文本内容是否匹配内容过滤规则（使用缓存设置，避免数据库查询）
    /// 设置项：
    ///   - `content_filter_enabled`: "true"/"false"（默认 false）
    ///   - `content_filter_rules`: 换行分隔的正则表达式列表
    ///
    /// 任意一条规则匹配即排除该内容
    pub fn is_content_excluded_by_rules_cached(&self, content: &ClipboardContent, settings: &HashMap<String, String>) -> bool {
        let enabled = settings.get("content_filter_enabled")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !enabled {
            return false;
        }

        let text = match content {
            ClipboardContent::Text(t) => t.as_str(),
            ClipboardContent::Html { text: Some(t), .. } => t.as_str(),
            ClipboardContent::Html { html, .. } => html.as_str(),
            ClipboardContent::Rtf { text: Some(t), .. } => t.as_str(),
            _ => return false,
        };

        let rules = match settings.get("content_filter_rules") {
            Some(s) if !s.is_empty() => s.as_str(),
            _ => return false,
        };

        let mut cache = self.content_filter_cache.lock();
        if !cache.update_if_changed(rules) {
            return false;
        }

        if let Some(matched) = cache.is_match(text) {
            debug!(
                "内容被过滤规则排除: {:?} (文本长度={})",
                matched,
                text.len()
            );
            return true;
        }

        false
    }

    /// 处理剪贴板内容，去重后存入数据库
    pub fn process(
        &self,
        content: ClipboardContent,
        source: Option<SourceAppInfo>,
    ) -> Result<Option<i64>, String> {
        // 使用缓存的设置，仅在版本号变化时重新从数据库读取
        let settings = self.get_cached_settings();

        let max_content_size = settings
            .get("max_content_size_kb")
            .and_then(|s| s.parse::<usize>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(DEFAULT_MAX_CONTENT_SIZE);

        // 文本大小限制
        if max_content_size > 0 && Self::is_text_like_content(&content) {
            let content_size = self.get_content_size(&content);
            if content_size > max_content_size {
                warn!("Text size {} bytes exceeds max {} bytes, skipping", content_size, max_content_size);
                return Ok(None);
            }
        }

        // 图片大小限制
        if let ClipboardContent::Image(ref data) = content {
            let max_image = settings
                .get("max_image_size_kb")
                .and_then(|s| s.parse::<usize>().ok())
                .map(|kb| kb * 1024)
                .unwrap_or(0);
            if max_image > 0 && data.len() > max_image {
                warn!("Image size {} bytes exceeds max {} bytes, skipping", data.len(), max_image);
                return Ok(None);
            }
        }

        // 文件大小限制（预计算大小，避免后续 process_files 重复调用 fs::metadata）
        let mut precomputed_file_size: Option<i64> = None;
        if let ClipboardContent::Files(ref files) = content {
            let total = Self::sum_file_sizes(files);
            precomputed_file_size = Some(total);
            let max_file = settings
                .get("max_file_size_kb")
                .and_then(|s| s.parse::<usize>().ok())
                .map(|kb| kb * 1024)
                .unwrap_or(0);
            if max_file > 0 && (total as usize) > max_file {
                warn!("Files size {} bytes exceeds max {} bytes, skipping", total, max_file);
                return Ok(None);
            }
        }

        // 视频大小限制（预计算大小，避免后续 process_video 重复调用 fs::metadata）
        if let ClipboardContent::Video(ref files) = content {
            let total = Self::sum_file_sizes(files);
            precomputed_file_size = Some(total);
            let max_video = settings
                .get("max_video_size_kb")
                .and_then(|s| s.parse::<usize>().ok())
                .map(|kb| kb * 1024)
                .unwrap_or(0);
            if max_video > 0 && (total as usize) > max_video {
                warn!("Video size {} bytes exceeds max {} bytes, skipping", total, max_video);
                return Ok(None);
            }
        }

        let hashes = self.calculate_hashes(&content);
        let text_like = Self::is_text_like_content(&content);
        let dedup = match settings.get("dedup_strategy").map(|s| s.as_str()) {
            Some("ignore") => "ignore",
            Some("always_new") => "always_new",
            _ => "move_to_top",
        };
        let text_dedup_mode = match settings.get("text_dedup_mode").map(|s| s.as_str()) {
            Some("strict") => "strict",
            _ => "semantic",
        };
        let text_use_strict = text_like && text_dedup_mode == "strict";

        if dedup != "always_new"
            && if text_like {
                if text_use_strict {
                    self.repository.exists_by_hash(&hashes.content_hash)
                } else {
                    self.repository
                        .exists_by_semantic_hash(&hashes.semantic_hash)
                }
            } else {
                self.repository.exists_by_hash(&hashes.content_hash)
            }
            .map_err(|e| e.to_string())?
        {
            match dedup {
                "ignore" => {
                    debug!("Content already exists, ignoring (dedup=ignore)");
                    return Ok(None);
                }
                _ => {
                    // move_to_top: 更新访问时间并置顶
                    debug!("Content already exists, updating access time (dedup=move_to_top)");
                    return if text_like {
                        if text_use_strict {
                            self.repository
                                .touch_by_hash(&hashes.content_hash)
                                .map_err(|e| e.to_string())
                        } else {
                            self.repository
                                .touch_by_semantic_hash(&hashes.semantic_hash)
                                .map_err(|e| e.to_string())
                        }
                    } else {
                        self.repository
                            .touch_by_hash(&hashes.content_hash)
                            .map_err(|e| e.to_string())
                    };
                }
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

        let mut item = match content {
            ClipboardContent::Text(text) => self.process_text(text, &hashes, max_content_size)?,
            ClipboardContent::Html { html, text } => {
                self.process_html(html, text, &hashes, max_content_size)?
            }
            ClipboardContent::Rtf { rtf, text } => {
                self.process_rtf(rtf, text, &hashes, max_content_size)?
            }
            ClipboardContent::Image(data) => self.process_image(data, &hashes)?,
            ClipboardContent::Files(files) => self.process_files(files, &hashes, precomputed_file_size)?,
            ClipboardContent::Video(files) => self.process_video(files, &hashes, precomputed_file_size)?,
        };

        item.source_app_name = source_app_name;
        item.source_app_icon = source_app_icon;

        let log_type = format!("{:?}", item.content_type);
        let log_size = item.byte_size;
        let log_source = item.source_app_name.clone().unwrap_or_else(|| "unknown".to_string());

        let id = self.repository.insert(item).map_err(|e| e.to_string())?;
        info!(
            "Stored clipboard item: id={}, type={}, size={} bytes, source={}",
            id, log_type, log_size, log_source
        );

        // 执行最大历史数限制，清理旧图片（复用批量读取的 settings）
        let max_history_count = settings
            .get("max_history_count")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_HISTORY_COUNT);
        if max_history_count > 0 {
            match self.repository.enforce_max_count(max_history_count) {
                Ok((deleted, image_paths)) => {
                    super::cleanup_image_files(&image_paths);
                    if deleted > 0 {
                        debug!("Enforced max count: removed {} old items", deleted);
                    }
                }
                Err(e) => warn!("Failed to enforce max history count: {}", e),
            }
        }

        // 自动清理超过指定天数的旧记录（复用批量读取的 settings）
        let auto_cleanup_days = settings
            .get("auto_cleanup_days")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(DEFAULT_AUTO_CLEANUP_DAYS);
        if auto_cleanup_days > 0 {
            match self.repository.delete_older_than(auto_cleanup_days) {
                Ok((deleted, image_paths)) => {
                    super::cleanup_image_files(&image_paths);
                    if deleted > 0 {
                        info!("Auto-cleanup: removed {} items older than {} days", deleted, auto_cleanup_days);
                    }
                }
                Err(e) => warn!("Failed to auto-cleanup old items: {}", e),
            }
        }

        Ok(Some(id))
    }

    /// 获取缓存的过滤设置（供 monitor 调用，避免重复数据库查询）
    pub fn get_filter_settings(&self) -> Arc<HashMap<String, String>> {
        self.get_cached_settings()
    }

    /// 获取缓存的处理设置（版本号变化时重新从数据库读取）
    fn get_cached_settings(&self) -> Arc<HashMap<String, String>> {
        let current_version = SETTINGS_VERSION.load(std::sync::atomic::Ordering::Relaxed);
        let mut cache = self.settings_cache.lock();
        if cache.version == current_version && !cache.values.is_empty() {
            return cache.values.clone(); // Arc::clone，仅增加引用计数
        }

        // 缓存失效，重新从数据库批量读取（包含过滤设置）
        let values = self.settings_repo
            .get_multiple(&[
                "max_content_size_kb", "max_image_size_kb", "max_file_size_kb",
                "max_video_size_kb", "dedup_strategy", "text_dedup_mode",
                "max_history_count", "auto_cleanup_days",
                // 过滤相关设置（供 is_source_app_excluded / is_content_type_allowed / is_content_excluded_by_rules 使用）
                "monitor_types", "app_filter_enabled", "app_filter_mode",
                "app_filter_list", "content_filter_enabled", "content_filter_rules",
            ])
            .unwrap_or_default();

        let arc_values = Arc::new(values);
        cache.values = arc_values.clone();
        cache.version = current_version;
        arc_values
    }

    fn get_content_size(&self, content: &ClipboardContent) -> usize {
        match content {
            ClipboardContent::Text(text) => text.len(),
            ClipboardContent::Html { html, .. } => html.len(),
            ClipboardContent::Rtf { rtf, .. } => rtf.len(),
            ClipboardContent::Image(data) => data.len(),
            ClipboardContent::Files(files) | ClipboardContent::Video(files) => files.iter().map(|f| f.len()).sum(),
        }
    }

    fn is_text_like_content(content: &ClipboardContent) -> bool {
        matches!(
            content,
            ClipboardContent::Text(_) | ClipboardContent::Html { .. } | ClipboardContent::Rtf { .. }
        )
    }

    fn calculate_hashes(&self, content: &ClipboardContent) -> ContentHashes {
        let content_hash = self.calculate_hash(content);
        let semantic_hash = match content {
            ClipboardContent::Text(text) => {
                semantic_hash_from_text(text).unwrap_or_else(|| content_hash.clone())
            }
            ClipboardContent::Html { text, .. } => {
                compute_semantic_hash("html", text.as_deref(), &content_hash)
            }
            ClipboardContent::Rtf { text, .. } => {
                compute_semantic_hash("rtf", text.as_deref(), &content_hash)
            }
            ClipboardContent::Image(_) | ClipboardContent::Files(_) | ClipboardContent::Video(_) => content_hash.clone(),
        };

        ContentHashes {
            content_hash,
            semantic_hash,
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
            ClipboardContent::Video(files) => {
                hasher.update(b"video:");
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
        hashes: &ContentHashes,
        max_size: usize,
    ) -> Result<NewClipboardItem, String> {
        let byte_size = text.len() as i64;
        let char_count = Some(text.chars().count() as i64);
        let preview = Self::create_preview(&text);
        let text_content = truncate_content(text, max_size, "Text");

        Ok(NewClipboardItem {
            content_type: ContentType::Text,
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
        hashes: &ContentHashes,
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
            rtf_content: Some(rtf_content),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            char_count,
            ..Default::default()
        })
    }

    /// 处理图片内容：保存到磁盘并提取宽高元数据
    fn process_image(&self, data: Vec<u8>, hashes: &ContentHashes) -> Result<NewClipboardItem, String> {
        let byte_size = data.len() as i64;

        let filename = format!("{}.png", &hashes.content_hash[..16]);
        let image_path = self.images_path.join(&filename);
        let image_path_str = image_path.to_string_lossy().to_string();

        let (image_width, image_height) = self.extract_image_dimensions(&data)?;
        debug!(
            "Processing image: {}x{}, {} bytes, hash={}",
            image_width,
            image_height,
            byte_size,
            &hashes.content_hash[..16]
        );

        // 同步写入文件，确保插入数据库前文件已就绪（异步写入会引发竞态）
        if let Err(e) = std::fs::write(&image_path, &data) {
            return Err(format!("Failed to save image: {}", e));
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

    fn extract_image_dimensions(&self, data: &[u8]) -> Result<(i64, i64), String> {
        let (w, h) = ImageReader::new(std::io::Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| format!("Failed to guess image format: {}", e))?
            .into_dimensions()
            .map_err(|e| format!("Failed to read image dimensions: {}", e))?;

        Ok((w as i64, h as i64))
    }

    /// 参数 precomputed_size: 若已在大小限制检查中计算过文件总大小则直接复用
    fn process_files(&self, files: Vec<String>, hashes: &ContentHashes, precomputed_size: Option<i64>) -> Result<NewClipboardItem, String> {
        debug!("Processing {} file(s)", files.len());

        let byte_size = precomputed_size.unwrap_or_else(|| Self::sum_file_sizes(&files));

        let preview = if files.len() == 1 {
            files[0].clone()
        } else {
            format!("{} files", files.len())
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Files,
            file_paths: Some(files),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            ..Default::default()
        })
    }

    /// 参数 precomputed_size: 若已在大小限制检查中计算过文件总大小则直接复用
    fn process_video(&self, files: Vec<String>, hashes: &ContentHashes, precomputed_size: Option<i64>) -> Result<NewClipboardItem, String> {
        debug!("Processing {} video file(s)", files.len());

        let byte_size = precomputed_size.unwrap_or_else(|| Self::sum_file_sizes(&files));

        let preview = if files.len() == 1 {
            files[0].clone()
        } else {
            format!("{} videos", files.len())
        };

        Ok(NewClipboardItem {
            content_type: ContentType::Video,
            file_paths: Some(files),
            content_hash: hashes.content_hash.clone(),
            semantic_hash: hashes.semantic_hash.clone(),
            preview: Some(preview),
            byte_size,
            ..Default::default()
        })
    }

    /// 计算文件列表中普通文件的总大小（跳过目录）
    fn sum_file_sizes(files: &[String]) -> i64 {
        files
            .iter()
            .filter_map(|f| {
                let path = std::path::Path::new(f);
                if path.is_file() {
                    std::fs::metadata(path).ok().map(|m| m.len() as i64)
                } else {
                    None
                }
            })
            .sum()
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
