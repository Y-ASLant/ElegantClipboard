//! WebDAV 同步模块
//!
//! 将剪贴板数据打包为 ZIP，上传/下载到 WebDAV 服务器。
//! 每次同步均为覆盖写入，避免远端文件无限增长。

use base64::Engine;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// WebDAV 连接配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    /// 远端目录，如 `/elegant-clipboard/`
    pub remote_dir: String,
    /// 代理模式: "system"(系统代理), "none"(不使用代理), "custom"(自定义代理)
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: String,
    /// 自定义代理地址，如 `http://127.0.0.1:7890` 或 `socks5://127.0.0.1:1080`
    #[serde(default)]
    pub proxy_url: String,
}

/// 同步选项
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncOptions {
    pub sync_text: bool,
    pub sync_image: bool,
    pub sync_files: bool,
    pub sync_video: bool,
    pub sync_settings: bool,
    /// 图片同步最大大小（KB），0 表示不限
    pub max_image_size_kb: u64,
    /// 文件同步最大大小（KB），0 表示不限
    pub max_file_size_kb: u64,
    /// 视频同步最大大小（KB），0 表示不限
    pub max_video_size_kb: u64,
}

impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            sync_text: true,
            sync_image: true,
            sync_files: true,
            sync_video: false,
            sync_settings: true,
            max_image_size_kb: 5120,
            max_file_size_kb: 5120,
            max_video_size_kb: 5120,
        }
    }
}


const SYNC_FILENAME: &str = "clipboard_sync.zip";

/// 媒体文件映射条目（记录每个文件的 hash 和本地路径，用于下载时定位）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaEntry {
    /// 文件内容的 blake3 hash
    pub hash: String,
    /// 文件扩展名
    pub ext: String,
    /// "image"、"file" 或 "video"
    pub media_type: String,
    /// 本地路径（image 为相对 images 目录的路径，file 为原始绝对路径）
    pub local_path: String,
    /// 来源设备标识（多设备安全清理用）
    #[serde(default)]
    pub device_id: String,
}

/// 获取或创建设备唯一标识（存储在 settings 表中）
pub fn get_or_create_device_id(db: &crate::database::Database) -> String {
    let repo = crate::database::SettingsRepository::new(db);
    if let Ok(Some(id)) = repo.get("device_id") {
        if !id.is_empty() {
            return id;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = repo.set("device_id", &id);
    info!("生成新设备标识: {}", id);
    id
}

/// 计算文件内容的 blake3 hash（hex）
fn file_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// 构建 Basic Auth 头
fn basic_auth(username: &str, password: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
    format!("Basic {}", encoded)
}

/// 规范化远端 URL（确保以 `/` 结尾）
fn normalize_url(base_url: &str, remote_dir: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let dir = remote_dir.trim_matches('/');
    if dir.is_empty() {
        format!("{}/", base)
    } else {
        format!("{}/{}/", base, dir)
    }
}

fn default_proxy_mode() -> String { "system".to_string() }

/// 构建 HTTP 客户端（根据配置决定代理模式）
/// - system: 使用系统代理（Windows 注册表 + 环境变量）
/// - none: 不使用任何代理
/// - custom: 使用自定义代理地址（支持 http/https/socks5）
fn build_client(config: &WebDavConfig) -> Result<reqwest::blocking::Client, String> {
    let builder = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60))
        .danger_accept_invalid_certs(true);

    let builder = crate::proxy::apply_proxy(builder, &config.proxy_mode, &config.proxy_url)?;

    builder.build().map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

/// 测试 WebDAV 连接
pub fn test_connection(config: &WebDavConfig) -> Result<String, String> {
    let client = build_client(config)?;
    let url = normalize_url(&config.url, &config.remote_dir);
    let auth = basic_auth(&config.username, &config.password);

    let resp = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
        .header("Authorization", &auth)
        .header("Depth", "0")
        .send()
        .map_err(|e| format!("连接失败: {}", e))?;

    let status = resp.status().as_u16();
    match status {
        200..=299 => Ok("连接成功".to_string()),
        401 => Err("认证失败，请检查用户名和密码".to_string()),
        403 => Err("无权限访问该目录".to_string()),
        404 => {
            // 尝试创建目录
            ensure_remote_dir(&client, &config.url, &config.remote_dir, &auth)?;
            Ok("连接成功（已创建远端目录）".to_string())
        }
        _ => Err(format!("服务器返回 HTTP {}", status)),
    }
}

/// 确保远端目录存在（MKCOL）
fn ensure_remote_dir(
    client: &reqwest::blocking::Client,
    base_url: &str,
    remote_dir: &str,
    auth: &str,
) -> Result<(), String> {
    let dir = remote_dir.trim_matches('/');
    if dir.is_empty() {
        return Ok(());
    }

    // 逐级创建目录
    let base = base_url.trim_end_matches('/');
    let mut path = String::new();
    for segment in dir.split('/') {
        if segment.is_empty() {
            continue;
        }
        path = if path.is_empty() {
            segment.to_string()
        } else {
            format!("{}/{}", path, segment)
        };
        let dir_url = format!("{}/{}/", base, path);
        let resp = client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &dir_url)
            .header("Authorization", auth)
            .send()
            .map_err(|e| format!("创建目录失败: {}", e))?;

        let status = resp.status().as_u16();
        // 201 Created, 405 已存在
        if status != 201 && status != 405 && !(200..=299).contains(&status) {
            debug!("MKCOL {} -> HTTP {}", dir_url, status);
        }
    }
    Ok(())
}

/// 计算 max_byte_size
pub fn calc_max_byte_size(max_size_kb: u64) -> i64 {
    if max_size_kb > 0 {
        (max_size_kb * 1024) as i64
    } else {
        i64::MAX
    }
}

/// 取所有类型限制的最大值，用于 SQL 粗筛
pub fn calc_max_query_size(options: &SyncOptions) -> i64 {
    let vals = [
        if options.sync_image { calc_max_byte_size(options.max_image_size_kb) } else { 0 },
        if options.sync_files { calc_max_byte_size(options.max_file_size_kb) } else { 0 },
        if options.sync_video { calc_max_byte_size(options.max_video_size_kb) } else { 0 },
    ];
    vals.into_iter().max().unwrap_or(i64::MAX).max(
        // 文本条目不受文件大小限制
        if options.sync_text { i64::MAX } else { 0 }
    )
}

/// 构建内容类型 SQL 过滤片段
pub fn build_type_filter(options: &SyncOptions) -> Vec<&'static str> {
    let mut types = Vec::new();
    if options.sync_text {
        types.push("'text'");
        types.push("'html'");
        types.push("'rtf'");
    }
    if options.sync_image {
        types.push("'image'");
    }
    if options.sync_files {
        types.push("'files'");
    }
    if options.sync_video {
        types.push("'video'");
    }
    types
}

/// 导出同步 ZIP（设置 + 条目元数据 + 媒体映射表，不含二进制文件）
pub fn export_sync_data(
    db: &crate::database::Database,
    data_dir: &Path,
    options: &SyncOptions,
) -> Result<Vec<u8>, String> {
    use std::io::Cursor;

    let buf = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let zip_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // 使用所有类型限制中的最大值作为粗筛，细筛在 build_media_map 中按类型单独限制
    let max_byte_size = calc_max_query_size(options);

    // 导出设置
    if options.sync_settings {
        let settings_repo = crate::database::SettingsRepository::new(db);
        if let Ok(all_settings) = settings_repo.get_all() {
            let json = serde_json::to_string_pretty(&all_settings)
                .map_err(|e| format!("序列化设置失败: {}", e))?;
            zip.start_file("settings.json", zip_options)
                .map_err(|e| e.to_string())?;
            zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    let content_types = build_type_filter(options);
    if content_types.is_empty() {
        let result = zip.finish().map_err(|e| e.to_string())?;
        return Ok(result.into_inner());
    }

    let type_filter = content_types.join(",");
    let repo = crate::database::ClipboardRepository::new(db);
    let items = repo
        .query_items_for_sync(&type_filter, max_byte_size)
        .map_err(|e| format!("查询条目失败: {}", e))?;

    info!("轻量同步导出: {} 条记录", items.len());

    let json = serde_json::to_string_pretty(&items)
        .map_err(|e| format!("序列化条目失败: {}", e))?;
    zip.start_file("items.json", zip_options)
        .map_err(|e| e.to_string())?;
    zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    // 导出标签及其条目关联
    let tag_repo = crate::database::TagRepository::new(db);
    if let Ok(tags_data) = tag_repo.export_tags_sync_data() {
        if !tags_data.tags.is_empty() {
            let tags_json = serde_json::to_string_pretty(&tags_data)
                .map_err(|e| format!("序列化标签失败: {}", e))?;
            zip.start_file("tags.json", zip_options)
                .map_err(|e| e.to_string())?;
            zip.write_all(tags_json.as_bytes()).map_err(|e| e.to_string())?;
            info!("标签同步导出: {} 个标签, {} 条关联", tags_data.tags.len(), tags_data.associations.len());
        }
    }

    // 构建媒体映射表（hash → 本地路径），用于下载时定位文件
    let device_id = get_or_create_device_id(db);
    let media_map = build_media_map(&items, data_dir, options, &device_id);
    if !media_map.is_empty() {
        let map_json = serde_json::to_string_pretty(&media_map)
            .map_err(|e| format!("序列化媒体映射失败: {}", e))?;
        zip.start_file("media_map.json", zip_options)
            .map_err(|e| e.to_string())?;
        zip.write_all(map_json.as_bytes()).map_err(|e| e.to_string())?;
        info!("媒体映射: {} 条（已去重）", media_map.len());
    }

    let result = zip.finish().map_err(|e| e.to_string())?;
    Ok(result.into_inner())
}

/// 根据条目构建媒体映射表（自动按 hash 去重）
pub fn build_media_map(
    items: &[crate::database::ClipboardItem],
    data_dir: &Path,
    options: &SyncOptions,
    device_id: &str,
) -> Vec<MediaEntry> {
    let images_dir = data_dir.join("images");
    let mut seen_hashes = std::collections::HashSet::new();
    let mut map = Vec::new();

    let max_image_bytes = calc_max_byte_size(options.max_image_size_kb);
    let max_file_bytes = calc_max_byte_size(options.max_file_size_kb);
    let max_video_bytes = calc_max_byte_size(options.max_video_size_kb);

    // 图片
    if options.sync_image {
        for item in items {
            if item.content_type == "image" {
                if let Some(ref img_path) = item.image_path {
                    let full_path = if Path::new(img_path).is_absolute() {
                        PathBuf::from(img_path)
                    } else {
                        images_dir.join(img_path)
                    };
                    if let Ok(data) = std::fs::read(&full_path) {
                        if (data.len() as i64) <= max_image_bytes {
                            let hash = file_hash(&data);
                            if seen_hashes.insert(hash.clone()) {
                                let ext = full_path.extension()
                                    .unwrap_or_default().to_string_lossy().to_string();
                                map.push(MediaEntry {
                                    hash,
                                    ext,
                                    media_type: "image".to_string(),
                                    local_path: img_path.clone(),
                                    device_id: device_id.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // 文件/视频（跳过已失效的条目）
    if options.sync_files || options.sync_video {
        for item in items {
            let is_video = item.content_type == "video";
            let is_files = item.content_type == "files";
            if (!is_video && !is_files) || item.files_valid == Some(false) {
                continue;
            }
            if is_video && !options.sync_video {
                continue;
            }
            if is_files && !options.sync_files {
                continue;
            }

            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json).unwrap_or_default();
                let limit = if is_video { max_video_bytes } else { max_file_bytes };

                for file_path in &paths {
                    let p = Path::new(file_path);
                    if let Ok(data) = std::fs::read(p) {
                        if (data.len() as i64) <= limit {
                            let hash = file_hash(&data);
                            if seen_hashes.insert(hash.clone()) {
                                let ext = p.extension()
                                    .unwrap_or_default().to_string_lossy().to_string();
                                map.push(MediaEntry {
                                    hash,
                                    ext,
                                    media_type: if is_video { "video" } else { "file" }.to_string(),
                                    local_path: file_path.clone(),
                                    device_id: device_id.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    map
}

/// 上传媒体文件到 WebDAV（逐个上传，hash 去重）
/// 返回 (上传数, 跳过数, 总大小)
pub fn upload_media_files(
    config: &WebDavConfig,
    entries: &[MediaEntry],
    data_dir: &Path,
) -> Result<(usize, usize, u64), String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let images_dir = data_dir.join("images");

    let mut uploaded = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;

    // 确保远端 media/ 目录存在（只需一次）
    let media_dir = format!("{}/media", config.remote_dir.trim_matches('/'));
    ensure_remote_dir(&client, &config.url, &media_dir, &auth)?;

    for entry in entries {
        let remote_path = format!("media/{}.{}", entry.hash, entry.ext);
        let remote_url = format!("{}{}", base_url, remote_path);

        // HEAD 检查远端是否已存在（hash 相同则内容相同，跳过）
        let exists = client.head(&remote_url)
            .header("Authorization", &auth)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        if exists {
            skipped += 1;
            continue;
        }

        // 读取本地文件
        let local_path = if entry.media_type == "image" {
            if Path::new(&entry.local_path).is_absolute() {
                PathBuf::from(&entry.local_path)
            } else {
                images_dir.join(&entry.local_path)
            }
        } else {
            PathBuf::from(&entry.local_path)
        };

        if let Ok(data) = std::fs::read(&local_path) {
            let resp = client.put(&remote_url)
                .header("Authorization", &auth)
                .header("Content-Type", "application/octet-stream")
                .body(data.to_vec())
                .send()
                .map_err(|e| format!("上传 {} 失败: {}", remote_path, e))?;

            if resp.status().is_success() {
                total_bytes += data.len() as u64;
                uploaded += 1;
            }
        }
    }

    info!("媒体上传: {} 个新文件, {} 个已存在跳过, 共 {} bytes", uploaded, skipped, total_bytes);
    Ok((uploaded, skipped, total_bytes))
}

/// 下载缺失的媒体文件（检查本地路径，不存在则从 WebDAV 下载到对应位置）
/// 返回下载数量
pub fn download_missing_media(
    config: &WebDavConfig,
    entries: &[MediaEntry],
    data_dir: &Path,
) -> Result<usize, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let images_dir = data_dir.join("images");
    let _ = std::fs::create_dir_all(&images_dir);

    let mut downloaded = 0usize;

    for entry in entries {
        // 确定本地目标路径
        let local_path = if entry.media_type == "image" {
            if Path::new(&entry.local_path).is_absolute() {
                PathBuf::from(&entry.local_path)
            } else {
                images_dir.join(&entry.local_path)
            }
        } else {
            PathBuf::from(&entry.local_path)
        };

        // 如果本地已存在则跳过
        if local_path.exists() {
            continue;
        }

        // 从 WebDAV 下载
        let remote_path = format!("media/{}.{}", entry.hash, entry.ext);
        let remote_url = format!("{}{}", base_url, remote_path);

        let resp = client.get(&remote_url)
            .header("Authorization", &auth)
            .send()
            .map_err(|e| format!("下载 {} 失败: {}", remote_path, e))?;

        if resp.status().is_success() {
            let bytes = resp.bytes().map_err(|e| e.to_string())?;
            // 确保父目录存在
            if let Some(parent) = local_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&local_path, &bytes).is_ok() {
                downloaded += 1;
                info!("媒体下载: {} -> {}", remote_path, local_path.display());
            }
        }
    }

    info!("媒体下载完成: {} 个文件", downloaded);
    Ok(downloaded)
}

/// 从同步 ZIP 导入（设置 + 条目元数据），同时返回媒体映射表
pub fn import_sync_data(
    db: &crate::database::Database,
    zip_data: &[u8],
    options: &SyncOptions,
) -> Result<ImportResult, String> {
    use std::io::Cursor;

    let reader = Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("读取 ZIP 失败: {}", e))?;

    let mut result = ImportResult::default();

    // 导入设置
    if options.sync_settings {
        if let Ok(mut entry) = archive.by_name("settings.json") {
            let mut json = String::new();
            entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
            if let Ok(settings) = serde_json::from_str::<std::collections::HashMap<String, String>>(&json) {
                let settings_repo = crate::database::SettingsRepository::new(db);
                let skip_keys: std::collections::HashSet<&str> = [
                    "webdav_url", "webdav_username", "webdav_password",
                    "webdav_remote_dir", "webdav_enabled", "webdav_auto_sync",
                    "webdav_sync_interval", "webdav_sync_text", "webdav_sync_image",
                    "webdav_sync_files", "webdav_sync_video", "webdav_sync_settings",
                    "webdav_max_image_size_kb", "webdav_max_file_size_kb", "webdav_max_video_size_kb",
                    "webdav_last_sync_time", "webdav_proxy_mode", "webdav_proxy_url",
                    "device_id",
                ].into_iter().collect();

                for (key, value) in &settings {
                    if !skip_keys.contains(key.as_str()) {
                        let _ = settings_repo.set(key, value);
                    }
                }
                result.settings_imported = true;
                info!("同步导入: 设置已恢复");
            }
        }
    }

    // 导入条目
    if let Ok(mut entry) = archive.by_name("items.json") {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        let items: Vec<crate::database::ClipboardItem> =
            serde_json::from_str(&json).map_err(|e| format!("解析条目失败: {}", e))?;

        let repo = crate::database::ClipboardRepository::new(db);
        let imported = repo
            .import_sync_items(&items)
            .map_err(|e| format!("导入条目失败: {}", e))?;
        result.items_imported = imported;
        info!("同步导入: {} 条记录", imported);
    }

    // 导入标签及关联（必须在条目导入之后，这样 content_hash 才能匹配到本地条目）
    if let Ok(mut entry) = archive.by_name("tags.json") {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        if let Ok(tags_data) = serde_json::from_str::<crate::database::TagsSyncData>(&json) {
            let tag_repo = crate::database::TagRepository::new(db);
            match tag_repo.import_tags_sync_data(&tags_data) {
                Ok(imported) => info!("同步导入: {} 个新标签, 关联已恢复", imported),
                Err(e) => info!("标签导入失败: {}", e),
            }
        }
    }

    // 提取媒体映射表
    if let Ok(mut entry) = archive.by_name("media_map.json") {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        if let Ok(map) = serde_json::from_str::<Vec<MediaEntry>>(&json) {
            result.media_map = map;
        }
    }

    Ok(result)
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportResult {
    pub settings_imported: bool,
    pub items_imported: usize,
    #[serde(skip)]
    pub media_map: Vec<MediaEntry>,
}

/// 从 WebDAV 下载独立的 media_map.json（权威媒体映射表）
pub fn download_media_map(config: &WebDavConfig) -> Result<Vec<MediaEntry>, String> {
    match download_sync(config, "media_map.json")? {
        Some(data) => {
            let json = String::from_utf8(data).map_err(|e| format!("解析 UTF-8 失败: {}", e))?;
            let map: Vec<MediaEntry> = serde_json::from_str(&json)
                .map_err(|e| format!("解析 media_map.json 失败: {}", e))?;
            info!("下载 media_map.json: {} 条", map.len());
            Ok(map)
        }
        None => {
            info!("远端无 media_map.json");
            Ok(Vec::new())
        }
    }
}

/// 上传 media_map.json（多设备安全合并）
///
/// 1. 移除当前设备不再引用的旧条目
/// 2. 添加当前设备的新条目
/// 3. 保留其他设备的所有条目不变
/// 返回更新后的完整映射（供后续清理使用）
pub fn upload_media_map(config: &WebDavConfig, local_entries: &[MediaEntry], device_id: &str) -> Result<Vec<MediaEntry>, String> {
    // 下载已有的远端映射
    let mut map = download_media_map(config).unwrap_or_default();

    // 移除当前设备的旧条目（含 device_id 匹配的，以及无 device_id 的旧数据）
    let local_hashes: std::collections::HashSet<&str> =
        local_entries.iter().map(|e| e.hash.as_str()).collect();
    let before = map.len();
    map.retain(|e| {
        if e.device_id == device_id || e.device_id.is_empty() {
            // 当前设备（或旧格式无标识）的条目：仅保留仍在本地引用的
            local_hashes.contains(e.hash.as_str())
        } else {
            // 其他设备的条目：始终保留
            true
        }
    });
    let removed = before - map.len();

    // 添加当前设备的新条目（按 hash+device_id 去重）
    let existing: std::collections::HashSet<(String, String)> =
        map.iter().map(|e| (e.hash.clone(), e.device_id.clone())).collect();
    let mut added = 0usize;
    for entry in local_entries {
        if !existing.contains(&(entry.hash.clone(), entry.device_id.clone())) {
            map.push(entry.clone());
            added += 1;
        }
    }

    // 上传合并后的映射
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    upload_sync(config, json.as_bytes(), "media_map.json")?;
    if added > 0 || removed > 0 {
        info!("上传 media_map.json: {} 条 (新增 {}, 移除 {})", map.len(), added, removed);
    }
    Ok(map)
}

/// 通过 PROPFIND 列出远端 media/ 目录下的文件名
fn list_remote_media_files(config: &WebDavConfig) -> Result<Vec<String>, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let media_url = format!("{}media/", base_url);

    let resp = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &media_url)
        .header("Authorization", &auth)
        .header("Depth", "1")
        .send()
        .map_err(|e| format!("PROPFIND media/ 失败: {}", e))?;

    if !resp.status().is_success() {
        return Ok(Vec::new());
    }

    let body = resp.text().map_err(|e| format!("读取 PROPFIND 响应失败: {}", e))?;
    let lower = body.to_lowercase();

    // 扫描整个响应体，提取所有 <d:href>...</d:href> 或 <href>...</href> 中的文件名
    let mut files = Vec::new();
    let open_tags = ["<d:href>", "<href>"];
    let close_tags = ["</d:href>", "</href>"];

    for (open, close) in open_tags.iter().zip(close_tags.iter()) {
        let open_len = open.len();
        let mut pos = 0;
        while let Some(start) = lower[pos..].find(open) {
            let abs_start = pos + start + open_len;
            if let Some(end) = lower[abs_start..].find(close) {
                let href = &body[abs_start..abs_start + end];
                let decoded = percent_decode(href);
                if let Some(name) = decoded.trim_end_matches('/').rsplit('/').next() {
                    if name.contains('.') {
                        files.push(name.to_string());
                    }
                }
                pos = abs_start + end + close.len();
            } else {
                break;
            }
        }
    }

    debug!("PROPFIND media/: 发现 {} 个文件", files.len());
    Ok(files)
}

/// URL 百分号解码
fn percent_decode(input: &str) -> String {
    let mut result = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(
                &input[i + 1..i + 3], 16,
            ) {
                result.push(val);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

/// 清理云端不再被任何设备引用的媒体文件
/// 基于 merged_map（所有设备引用的并集）判定孤立文件，避免误删其他设备仍需要的文件
/// 返回删除的文件数量
pub fn cleanup_orphaned_remote_media(
    config: &WebDavConfig,
    merged_map: &[MediaEntry],
) -> Result<usize, String> {
    let remote_files = list_remote_media_files(config)?;
    if remote_files.is_empty() {
        return Ok(0);
    }

    // 所有设备引用的 hash 集合
    let referenced_hashes: std::collections::HashSet<&str> =
        merged_map.iter().map(|e| e.hash.as_str()).collect();

    // 从远端文件名提取 hash（文件名格式: {hash}.{ext}）
    let orphan_files: Vec<&String> = remote_files.iter().filter(|f| {
        if let Some(hash) = f.rsplit('.').last() {
            !referenced_hashes.contains(hash)
        } else {
            false
        }
    }).collect();

    if orphan_files.is_empty() {
        return Ok(0);
    }

    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let mut deleted = 0usize;

    for filename in &orphan_files {
        let remote_url = format!("{}media/{}", base_url, filename);

        let resp = client
            .delete(&remote_url)
            .header("Authorization", &auth)
            .send();

        match resp {
            Ok(r) if r.status().is_success() => {
                deleted += 1;
                info!("清理云端孤立媒体: media/{}", filename);
            }
            _ => {}
        }
    }

    // 同步更新 media_map.json（移除已删除条目的引用）
    let orphan_hashes: std::collections::HashSet<&str> = orphan_files.iter().filter_map(|f| {
        f.rsplit('.').last()
    }).collect();
    if !orphan_hashes.is_empty() {
        if let Ok(mut map) = download_media_map(config) {
            let before = map.len();
            map.retain(|e| !orphan_hashes.contains(e.hash.as_str()));
            if map.len() < before {
                let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
                let _ = upload_sync(config, json.as_bytes(), "media_map.json");
            }
        }
    }

    info!("云端媒体清理完成: 删除 {} 个", deleted);
    Ok(deleted)
}

/// 上传数据到 WebDAV
pub fn upload_sync(config: &WebDavConfig, data: &[u8], filename: &str) -> Result<(), String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);

    // 确保目录存在
    ensure_remote_dir(&client, &config.url, &config.remote_dir, &auth)?;

    let file_url = format!("{}{}", base_url, filename);
    info!("WebDAV 上传: {} ({} bytes)", file_url, data.len());

    let resp = client
        .put(&file_url)
        .header("Authorization", &auth)
        .header("Content-Type", "application/zip")
        .body(data.to_vec())
        .send()
        .map_err(|e| format!("上传失败: {}", e))?;

    let status = resp.status().as_u16();
    if status >= 200 && status < 300 {
        info!("WebDAV 上传成功: HTTP {}", status);
        Ok(())
    } else {
        Err(format!("上传失败: HTTP {}", status))
    }
}

/// 从数据库加载 WebDAV 配置和同步选项
fn load_config_and_options(db: &crate::database::Database) -> Option<(WebDavConfig, SyncOptions)> {
    let repo = crate::database::SettingsRepository::new(db);
    let url = repo.get("webdav_url").ok().flatten().unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    let username = repo.get("webdav_username").ok().flatten().unwrap_or_default();
    let password = repo.get("webdav_password").ok().flatten().unwrap_or_default();
    let remote_dir = repo
        .get("webdav_remote_dir")
        .ok()
        .flatten()
        .unwrap_or_else(|| "/elegant-clipboard".to_string());

    let get_bool = |key: &str, default: bool| -> bool {
        repo.get(key).ok().flatten().map(|v| v != "false").unwrap_or(default)
    };
    let get_u64 = |key: &str, default: u64| -> u64 {
        repo.get(key).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(default)
    };
    let options = SyncOptions {
        sync_text: get_bool("webdav_sync_text", true),
        sync_image: get_bool("webdav_sync_image", true),
        sync_files: get_bool("webdav_sync_files", true),
        sync_video: get_bool("webdav_sync_video", false),
        sync_settings: true,
        max_image_size_kb: get_u64("webdav_max_image_size_kb", 5120),
        max_file_size_kb: get_u64("webdav_max_file_size_kb", 5120),
        max_video_size_kb: get_u64("webdav_max_video_size_kb", 5120),
    };

    let proxy_mode = repo.get("webdav_proxy_mode").ok().flatten().unwrap_or_else(|| "system".to_string());
    let proxy_url = repo.get("webdav_proxy_url").ok().flatten().unwrap_or_default();

    Some((WebDavConfig { url, username, password, remote_dir, proxy_mode, proxy_url }, options))
}

/// 用于追踪媒体同步是否正在进行
static MEDIA_SYNC_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 启动后台自动同步任务（在 app setup 中调用一次）
/// 轻量同步（元数据）按设定间隔频繁同步；媒体同步（图片/文件）在独立线程异步执行
pub fn start_auto_sync_task(db: crate::database::Database, data_dir: std::path::PathBuf) {
    std::thread::Builder::new()
        .name("webdav-auto-sync".into())
        .spawn(move || {
            // 等待应用启动完成
            std::thread::sleep(std::time::Duration::from_secs(30));

            let mut cycle_count: u64 = 0;

            loop {
                let settings_repo = crate::database::SettingsRepository::new(&db);
                let enabled = settings_repo
                    .get("webdav_enabled")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                let auto_sync = settings_repo
                    .get("webdav_auto_sync")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);

                if enabled && auto_sync {
                    let interval_secs: u64 = settings_repo
                        .get("webdav_sync_interval")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60);

                    if let Some((config, options)) = load_config_and_options(&db) {
                        // ── 轻量同步（元数据）── 导出并上传至云端
                        info!("WebDAV 轻量同步: 开始上传");

                        match export_sync_data(&db, &data_dir, &options) {
                            Ok(zip_data) => {
                                if let Err(e) = upload_sync(&config, &zip_data, SYNC_FILENAME) {
                                    info!("WebDAV 轻量同步上传失败: {}", e);
                                } else {
                                    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                                    let _ = settings_repo.set("webdav_last_sync_time", &now);
                                    info!("WebDAV 轻量同步: 上传完成");
                                }
                            }
                            Err(e) => info!("WebDAV 轻量同步导出失败: {}", e),
                        }

                        // ── 媒体同步（图片/文件分开异步）── 每 3 个周期执行一次
                        let need_media = (options.sync_image || options.sync_files || options.sync_video) && cycle_count % 3 == 0;
                        if need_media
                            && !MEDIA_SYNC_RUNNING.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            MEDIA_SYNC_RUNNING.store(true, std::sync::atomic::Ordering::Relaxed);

                            // 构建本地媒体映射
                            let device_id = get_or_create_device_id(&db);
                            let max_bs = calc_max_query_size(&options);
                            let content_types = build_type_filter(&options);
                            let local_items = if !content_types.is_empty() {
                                let tf = content_types.join(",");
                                crate::database::ClipboardRepository::new(&db)
                                    .query_items_for_sync(&tf, max_bs)
                                    .unwrap_or_default()
                            } else {
                                Vec::new()
                            };
                            let local_map = build_media_map(&local_items, &data_dir, &options, &device_id);

                            // 合并本地映射到远端 media_map.json（返回所有设备的完整映射）
                            let merged_map = if !local_map.is_empty() {
                                upload_media_map(&config, &local_map, &device_id).unwrap_or_default()
                            } else {
                                download_media_map(&config).unwrap_or_default()
                            };

                            // 清理云端不再被任何设备引用的媒体文件
                            let _ = cleanup_orphaned_remote_media(&config, &merged_map);

                            // 分离上传用的本地映射
                            let local_images: Vec<MediaEntry> = local_map.iter().filter(|e| e.media_type == "image").cloned().collect();
                            let local_files: Vec<MediaEntry> = local_map.iter().filter(|e| e.media_type == "file").cloned().collect();
                            let local_videos: Vec<MediaEntry> = local_map.iter().filter(|e| e.media_type == "video").cloned().collect();

                            // 仅下载本地数据库已有条目引用的媒体（避免下载未导入元数据的其他设备文件成为孤立文件）
                            let mut local_referenced_paths = std::collections::HashSet::new();
                            for item in &local_items {
                                if item.content_type == "image" {
                                    if let Some(ref p) = item.image_path {
                                        local_referenced_paths.insert(p.clone());
                                    }
                                }
                                if item.content_type == "files" || item.content_type == "video" {
                                    if let Some(ref paths_json) = item.file_paths {
                                        if let Ok(paths) = serde_json::from_str::<Vec<String>>(paths_json) {
                                            for p in paths {
                                                local_referenced_paths.insert(p);
                                            }
                                        }
                                    }
                                }
                            }
                            let invalid_paths = crate::database::ClipboardRepository::new(&db)
                                .get_invalid_file_paths_set();
                            let dl_images: Vec<MediaEntry> = merged_map.iter()
                                .filter(|e| e.media_type == "image" && local_referenced_paths.contains(&e.local_path))
                                .cloned().collect();
                            let dl_files: Vec<MediaEntry> = merged_map.iter()
                                .filter(|e| e.media_type == "file"
                                    && local_referenced_paths.contains(&e.local_path)
                                    && !invalid_paths.contains(&e.local_path))
                                .cloned().collect();
                            let dl_videos: Vec<MediaEntry> = merged_map.into_iter()
                                .filter(|e| e.media_type == "video"
                                    && local_referenced_paths.contains(&e.local_path)
                                    && !invalid_paths.contains(&e.local_path))
                                .collect();

                            // 用计数器追踪活跃线程数，全部完成后清除标志
                            let pending = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

                            // 图片同步线程
                            if options.sync_image && (!local_images.is_empty() || !dl_images.is_empty()) {
                                pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let cfg = config.clone();
                                let dir = data_dir.clone();
                                let cnt = pending.clone();
                                std::thread::Builder::new()
                                    .name("webdav-sync-images".into())
                                    .spawn(move || {
                                        if !local_images.is_empty() {
                                            match upload_media_files(&cfg, &local_images, &dir) {
                                                Ok((u, s, _)) => info!("图片上传: {} 新, {} 跳过", u, s),
                                                Err(e) => info!("图片上传失败: {}", e),
                                            }
                                        }
                                        if !dl_images.is_empty() {
                                            match download_missing_media(&cfg, &dl_images, &dir) {
                                                Ok(n) => { if n > 0 { info!("图片下载: {} 个", n); } }
                                                Err(e) => info!("图片下载失败: {}", e),
                                            }
                                        }
                                        if cnt.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                                            MEDIA_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    })
                                    .ok();
                            }

                            // 文件同步线程
                            if options.sync_files && (!local_files.is_empty() || !dl_files.is_empty()) {
                                pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let cfg = config.clone();
                                let dir = data_dir.clone();
                                let cnt = pending.clone();
                                std::thread::Builder::new()
                                    .name("webdav-sync-files".into())
                                    .spawn(move || {
                                        if !local_files.is_empty() {
                                            match upload_media_files(&cfg, &local_files, &dir) {
                                                Ok((u, s, _)) => info!("文件上传: {} 新, {} 跳过", u, s),
                                                Err(e) => info!("文件上传失败: {}", e),
                                            }
                                        }
                                        if !dl_files.is_empty() {
                                            match download_missing_media(&cfg, &dl_files, &dir) {
                                                Ok(n) => { if n > 0 { info!("文件下载: {} 个", n); } }
                                                Err(e) => info!("文件下载失败: {}", e),
                                            }
                                        }
                                        if cnt.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                                            MEDIA_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    })
                                    .ok();
                            }

                            // 视频同步线程
                            if options.sync_video && (!local_videos.is_empty() || !dl_videos.is_empty()) {
                                pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                let cfg = config.clone();
                                let dir = data_dir.clone();
                                let cnt = pending.clone();
                                std::thread::Builder::new()
                                    .name("webdav-sync-videos".into())
                                    .spawn(move || {
                                        if !local_videos.is_empty() {
                                            match upload_media_files(&cfg, &local_videos, &dir) {
                                                Ok((u, s, _)) => info!("视频上传: {} 新, {} 跳过", u, s),
                                                Err(e) => info!("视频上传失败: {}", e),
                                            }
                                        }
                                        if !dl_videos.is_empty() {
                                            match download_missing_media(&cfg, &dl_videos, &dir) {
                                                Ok(n) => { if n > 0 { info!("视频下载: {} 个", n); } }
                                                Err(e) => info!("视频下载失败: {}", e),
                                            }
                                        }
                                        if cnt.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                                            MEDIA_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    })
                                    .ok();
                            }

                            // 如果没有启动任何线程，立即清除标志
                            if pending.load(std::sync::atomic::Ordering::Relaxed) == 0 {
                                MEDIA_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                    }

                    cycle_count = cycle_count.wrapping_add(1);
                    std::thread::sleep(std::time::Duration::from_secs(interval_secs));
                } else {
                    cycle_count = 0;
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            }
        })
        .expect("failed to spawn webdav-auto-sync thread");
}

/// 从 WebDAV 下载 ZIP（通过 filename 区分元数据和媒体文件）
pub fn download_sync(config: &WebDavConfig, filename: &str) -> Result<Option<Vec<u8>>, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let file_url = format!("{}{}", base_url, filename);

    info!("WebDAV 下载: {}", file_url);

    let resp = client
        .get(&file_url)
        .header("Authorization", &auth)
        .send()
        .map_err(|e| format!("下载失败: {}", e))?;

    let status = resp.status().as_u16();
    match status {
        200..=299 => {
            let bytes = resp.bytes().map_err(|e| format!("读取响应失败: {}", e))?;
            info!("WebDAV 下载成功: {} bytes", bytes.len());
            Ok(Some(bytes.to_vec()))
        }
        404 => {
            info!("远端无同步文件");
            Ok(None)
        }
        _ => Err(format!("下载失败: HTTP {}", status)),
    }
}
