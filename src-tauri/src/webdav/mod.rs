//! WebDAV 同步模块
//!
//! 将剪贴板数据打包为 ZIP，上传/下载到 WebDAV 服务器。
//! 每次同步均为覆盖写入，避免远端文件无限增长。

use base64::Engine;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

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
    /// 是否接受无效 TLS 证书。默认关闭，仅用于自签名 WebDAV 服务兼容。
    #[serde(default)]
    pub accept_invalid_certs: bool,
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
const MAX_SYNC_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// 媒体文件映射条目（记录每个文件的 hash 和本地路径，用于下载时定位）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaEntry {
    /// 文件内容的 blake3 hash
    pub hash: String,
    /// 文件扩展名
    pub ext: String,
    /// "image"、"icon" 或 "file"
    pub media_type: String,
    /// 来源设备上的路径（仅用于与条目记录关联，不再决定下载写入位置）
    pub local_path: String,
    /// 来源设备标识（多设备安全清理用）
    #[serde(default)]
    pub device_id: String,
    /// 原始文件名（file 类型下载落地时命名用；旧版本数据无此字段）
    #[serde(default)]
    pub file_name: String,
    /// 文件大小（bytes；旧版本数据无此字段）
    #[serde(default)]
    pub size: u64,
    /// 本机实际可读的源文件路径（仅上传端本地使用，不写入 media_map.json）
    #[serde(skip)]
    pub source_path: Option<String>,
}

/// 后端返回给前端的结构化错误码（由前端 i18n 解析）
pub const SYNC_SESSION_BUSY: &str = "WEBDAV:SYNC_IN_PROGRESS";

/// 从任意来源设备的路径中提取文件名（兼容 Windows / Unix 分隔符）
fn file_name_from_any_path(path: &str) -> Option<String> {
    let name = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()?
        .trim();
    safe_media_file_name(name)
}

/// 校验媒体文件名，拒绝路径分隔符与 `..` 组件
fn safe_media_file_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return None;
    }
    Some(trimmed.to_string())
}

/// 计算媒体条目在本机数据目录中的落地路径。
///
/// 写入位置完全由本机数据目录 + 内容 hash 构造，与来源设备的路径无关，
/// 因此跨设备同步不再依赖两端目录一致。
pub fn local_media_target(entry: &MediaEntry, data_dir: &Path) -> Option<PathBuf> {
    if entry.hash.is_empty() {
        return None;
    }
    match entry.media_type.as_str() {
        "image" => {
            let ext = if entry.ext.is_empty() {
                "png"
            } else {
                &entry.ext
            };
            Some(
                data_dir
                    .join("images")
                    .join(format!("{}.{ext}", entry.hash)),
            )
        }
        // 图标保留原文件名（{cache_key}.png），使本机后续捕获能命中图标缓存
        "icon" => {
            let name = file_name_from_any_path(&entry.local_path)?;
            Some(data_dir.join("icons").join(name))
        }
        "file" => {
            let name = if entry.file_name.is_empty() {
                file_name_from_any_path(&entry.local_path)?
            } else {
                safe_media_file_name(&entry.file_name)?
            };
            let prefix = &entry.hash[..16.min(entry.hash.len())];
            Some(
                data_dir
                    .join("staged")
                    .join("webdav")
                    .join(format!("{prefix}_{name}")),
            )
        }
        _ => None,
    }
}

/// 获取或创建设备唯一标识（存储在 settings 表中）
pub fn get_or_create_device_id(db: &crate::database::Database) -> String {
    let repo = crate::database::SettingsRepository::new(db);
    if let Ok(Some(id)) = repo.get("device_id")
        && !id.is_empty()
    {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = repo.set("device_id", &id);
    info!("生成新设备标识: {}", id);
    id
}

/// 计算文件内容的 blake3 hash（hex）
fn file_hash_from_path(path: &Path) -> Result<(String, u64), String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("读取文件失败 {}: {}", path.display(), e))?;
    let mut hasher = blake3::Hasher::new();
    let bytes = std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("计算文件 hash 失败 {}: {}", path.display(), e))?;
    Ok((hasher.finalize().to_hex().to_string(), bytes))
}

fn file_len_if_within_limit(path: &Path, max_bytes: i64) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    if max_bytes >= 0 && len > max_bytes as u64 {
        return None;
    }
    Some(len)
}

/// 构建 Basic Auth 头
fn basic_auth(username: &str, password: &str) -> String {
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {encoded}")
}

/// 规范化远端 URL（确保以 `/` 结尾）
fn normalize_url(base_url: &str, remote_dir: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let dir = remote_dir.trim_matches('/');
    if dir.is_empty() {
        format!("{base}/")
    } else {
        format!("{base}/{dir}/")
    }
}

fn default_proxy_mode() -> String {
    "system".to_string()
}

/// 构建 HTTP 客户端（根据配置决定代理模式）
fn build_client(config: &WebDavConfig) -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60));

    if config.accept_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
        info!("WebDAV TLS 证书校验已关闭，仅用于自签名服务兼容");
    }

    let builder = crate::proxy::apply_proxy(builder, &config.proxy_mode, &config.proxy_url)?;

    builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
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
        .map_err(|e| format!("连接失败: {e}"))?;

    let status = resp.status().as_u16();
    match status {
        200..=299 => Ok("连接成功".to_string()),
        401 => Err("认证失败，请检查用户名和密码".to_string()),
        403 => Err("无权限访问该目录".to_string()),
        404 => {
            ensure_remote_dir(&client, &config.url, &config.remote_dir, &auth)?;
            Ok("连接成功（已创建远端目录）".to_string())
        }
        _ => Err(format!("服务器返回 HTTP {status}")),
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

    let base = base_url.trim_end_matches('/');
    let mut path = String::new();
    for segment in dir.split('/') {
        if segment.is_empty() {
            continue;
        }
        path = if path.is_empty() {
            segment.to_string()
        } else {
            format!("{path}/{segment}")
        };
        let dir_url = format!("{base}/{path}/");
        let resp = client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &dir_url)
            .header("Authorization", auth)
            .send()
            .map_err(|e| format!("创建目录失败: {e}"))?;

        let status = resp.status().as_u16();
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

/// 按同步选项计算各类型的条目查询限制。
/// 返回 (是否含文本, 图片大小上限, 文件大小上限)，`None` 表示该类型不同步。
pub fn sync_query_limits(options: &SyncOptions) -> (bool, Option<i64>, Option<i64>) {
    (
        options.sync_text,
        options
            .sync_image
            .then(|| calc_max_byte_size(options.max_image_size_kb)),
        options
            .sync_files
            .then(|| calc_max_byte_size(options.max_file_size_kb)),
    )
}

/// 查询符合同步条件的条目（超过大小限制的图片/文件条目不再导出）
pub fn query_sync_items(
    db: &crate::database::Database,
    options: &SyncOptions,
) -> Result<Vec<crate::database::ClipboardItem>, String> {
    let (include_text, image_max, files_max) = sync_query_limits(options);
    crate::database::ClipboardRepository::new(db)
        .query_items_for_sync(include_text, image_max, files_max)
        .map_err(|e| format!("查询条目失败: {e}"))
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

    if options.sync_settings {
        let settings_repo = crate::database::SettingsRepository::new(db);
        if let Ok(all_settings) = settings_repo.get_all() {
            let json = serde_json::to_string_pretty(&all_settings)
                .map_err(|e| format!("序列化设置失败: {e}"))?;
            zip.start_file("settings.json", zip_options)
                .map_err(|e| e.to_string())?;
            zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    if !options.sync_text && !options.sync_image && !options.sync_files {
        let result = zip.finish().map_err(|e| e.to_string())?;
        return Ok(result.into_inner());
    }

    let items = query_sync_items(db, options)?;

    info!("轻量同步导出: {} 条记录", items.len());

    let json = serde_json::to_string_pretty(&items).map_err(|e| format!("序列化条目失败: {e}"))?;
    zip.start_file("items.json", zip_options)
        .map_err(|e| e.to_string())?;
    zip.write_all(json.as_bytes()).map_err(|e| e.to_string())?;

    let device_id = get_or_create_device_id(db);
    let media_map = build_media_map(&items, data_dir, options, &device_id);
    if !media_map.is_empty() {
        let map_json = serde_json::to_string_pretty(&media_map)
            .map_err(|e| format!("序列化媒体映射失败: {e}"))?;
        zip.start_file("media_map.json", zip_options)
            .map_err(|e| e.to_string())?;
        zip.write_all(map_json.as_bytes())
            .map_err(|e| e.to_string())?;
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
    let icons_dir = data_dir.join("icons");
    let mut seen_hashes = std::collections::HashSet::new();
    let mut map = Vec::new();

    let max_image_bytes = calc_max_byte_size(options.max_image_size_kb);
    let max_file_bytes = calc_max_byte_size(options.max_file_size_kb);

    if options.sync_image {
        for item in items {
            if item.content_type == "image"
                && let Some(ref img_path) = item.image_path
            {
                let full_path = if Path::new(img_path).is_absolute() {
                    PathBuf::from(img_path)
                } else {
                    images_dir.join(img_path)
                };
                if let Some(size) = file_len_if_within_limit(&full_path, max_image_bytes)
                    && let Ok((hash, _)) = file_hash_from_path(&full_path)
                    && seen_hashes.insert(hash.clone())
                {
                    let ext = full_path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    map.push(MediaEntry {
                        hash,
                        ext,
                        media_type: "image".to_string(),
                        local_path: img_path.clone(),
                        device_id: device_id.to_string(),
                        file_name: String::new(),
                        size,
                        source_path: Some(full_path.to_string_lossy().to_string()),
                    });
                }
            }
        }
    }

    // 应用图标（始终同步，体积极小）
    {
        let mut seen_icon_paths = std::collections::HashSet::new();
        for item in items {
            if let Some(ref icon_path) = item.source_app_icon {
                if !seen_icon_paths.insert(icon_path.clone()) {
                    continue;
                }
                let full_path = if Path::new(icon_path).is_absolute() {
                    PathBuf::from(icon_path)
                } else {
                    icons_dir.join(icon_path)
                };
                if full_path.is_file()
                    && let Ok((hash, size)) = file_hash_from_path(&full_path)
                    && seen_hashes.insert(hash.clone())
                {
                    let ext = full_path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let file_name = full_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    map.push(MediaEntry {
                        hash,
                        ext,
                        media_type: "icon".to_string(),
                        local_path: icon_path.clone(),
                        device_id: device_id.to_string(),
                        file_name,
                        size,
                        source_path: Some(full_path.to_string_lossy().to_string()),
                    });
                }
            }
        }
    }

    if options.sync_files {
        for item in items {
            if item.content_type != "files" {
                continue;
            }
            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json).unwrap_or_default();
                // 原始路径不存在时回退到本机 staged 副本（跨设备导入的条目）
                let payload =
                    crate::clipboard::file_clipboard::decode_payload(item.file_payload.as_deref());
                let resolved =
                    crate::clipboard::file_clipboard::resolve_paths(&paths, payload.as_ref());
                for (file_path, resolved_path) in paths.iter().zip(resolved.iter()) {
                    let p = Path::new(resolved_path);
                    if let Some(size) = file_len_if_within_limit(p, max_file_bytes)
                        && let Ok((hash, _)) = file_hash_from_path(p)
                        && seen_hashes.insert(hash.clone())
                    {
                        let ext = p
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let file_name = p
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        map.push(MediaEntry {
                            hash,
                            ext,
                            media_type: "file".to_string(),
                            local_path: file_path.clone(),
                            device_id: device_id.to_string(),
                            file_name,
                            size,
                            source_path: Some(resolved_path.clone()),
                        });
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
    let icons_dir = data_dir.join("icons");

    let mut uploaded = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;

    let media_dir = format!("{}/media", config.remote_dir.trim_matches('/'));
    ensure_remote_dir(&client, &config.url, &media_dir, &auth)?;

    for entry in entries {
        let remote_path = format!("media/{}.{}", entry.hash, entry.ext);
        let remote_url = format!("{base_url}{remote_path}");

        let exists = client
            .head(&remote_url)
            .header("Authorization", &auth)
            .send()
            .is_ok_and(|r| r.status().is_success());

        if exists {
            skipped += 1;
            continue;
        }

        // 优先使用构建映射时解析出的实际可读路径
        let local_path = match entry.source_path {
            Some(ref p) => PathBuf::from(p),
            None => match entry.media_type.as_str() {
                "image" => {
                    if Path::new(&entry.local_path).is_absolute() {
                        PathBuf::from(&entry.local_path)
                    } else {
                        images_dir.join(&entry.local_path)
                    }
                }
                "icon" => {
                    if Path::new(&entry.local_path).is_absolute() {
                        PathBuf::from(&entry.local_path)
                    } else {
                        icons_dir.join(&entry.local_path)
                    }
                }
                _ => PathBuf::from(&entry.local_path),
            },
        };

        let file = match std::fs::File::open(&local_path) {
            Ok(file) => file,
            Err(e) => {
                debug!("媒体上传跳过，无法打开 {}: {}", local_path.display(), e);
                continue;
            }
        };
        let data_len = match file.metadata() {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                debug!(
                    "媒体上传跳过，无法读取 metadata {}: {}",
                    local_path.display(),
                    e
                );
                continue;
            }
        };
        let resp = client
            .put(&remote_url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", data_len)
            .body(reqwest::blocking::Body::new(file))
            .send()
            .map_err(|e| format!("上传 {remote_path} 失败: {e}"))?;

        if resp.status().is_success() {
            total_bytes += data_len;
            uploaded += 1;
        }
    }

    info!(
        "媒体上传: {} 个新文件, {} 个已存在跳过, 共 {} bytes",
        uploaded, skipped, total_bytes
    );
    Ok((uploaded, skipped, total_bytes))
}

/// 下载缺失的媒体文件（按内容 hash 落地到本机数据目录，与来源设备路径无关）
/// 返回下载数量
pub fn download_missing_media(
    config: &WebDavConfig,
    entries: &[MediaEntry],
    data_dir: &Path,
) -> Result<usize, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);

    let mut downloaded = 0usize;

    for entry in entries {
        let Some(local_path) = local_media_target(entry, data_dir) else {
            warn!(
                "跳过无法定位的媒体条目: {} ({})",
                entry.local_path, entry.media_type
            );
            continue;
        };

        if local_path.exists() {
            continue;
        }

        let remote_path = format!("media/{}.{}", entry.hash, entry.ext);
        let remote_url = format!("{base_url}{remote_path}");

        let resp = client
            .get(&remote_url)
            .header("Authorization", &auth)
            .send()
            .map_err(|e| format!("下载 {remote_path} 失败: {e}"))?;

        if resp.status().is_success() {
            if let Some(parent) = local_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp_path = local_path.with_extension(format!(
                "{}.download",
                local_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("tmp")
            ));
            let mut body = resp;
            match std::fs::File::create(&tmp_path)
                .and_then(|mut file| std::io::copy(&mut body, &mut file).map(|_| ()))
                .and_then(|_| std::fs::rename(&tmp_path, &local_path))
            {
                Ok(()) => {
                    downloaded += 1;
                    info!("媒体下载: {} -> {}", remote_path, local_path.display());
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    warn!("媒体下载写入失败 {}: {}", local_path.display(), e);
                }
            }
        }
    }

    info!("媒体下载完成: {} 个文件", downloaded);
    Ok(downloaded)
}

/// 从同步 ZIP 导入（设置 + 条目元数据）。
/// 导入前根据媒体映射表把跨设备条目的媒体路径改写为本机落地路径。
pub fn import_sync_data(
    db: &crate::database::Database,
    zip_data: &[u8],
    options: &SyncOptions,
    data_dir: &Path,
) -> Result<ImportResult, String> {
    use std::io::Cursor;

    let reader = Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("读取 ZIP 失败: {e}"))?;

    let mut result = ImportResult::default();

    if options.sync_settings
        && let Ok(mut entry) = archive.by_name("settings.json")
    {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        if let Ok(settings) =
            serde_json::from_str::<std::collections::HashMap<String, String>>(&json)
        {
            let settings_repo = crate::database::SettingsRepository::new(db);
            let skip_keys: std::collections::HashSet<&str> = [
                "webdav_url",
                "webdav_username",
                "webdav_password",
                "webdav_remote_dir",
                "webdav_enabled",
                "webdav_auto_sync",
                "webdav_sync_interval",
                "webdav_sync_text",
                "webdav_sync_image",
                "webdav_sync_files",
                "webdav_sync_video",
                "webdav_sync_settings",
                "webdav_max_image_size_kb",
                "webdav_max_file_size_kb",
                "webdav_max_video_size_kb",
                "webdav_last_sync_time",
                "webdav_proxy_mode",
                "webdav_proxy_url",
                "device_id",
            ]
            .into_iter()
            .collect();

            for (key, value) in &settings {
                if !skip_keys.contains(key.as_str()) {
                    let _ = settings_repo.set(key, value);
                }
            }
            result.settings_imported = true;
            info!("同步导入: 设置已恢复");
        }
    }

    // 先读媒体映射表，供条目导入时改写路径
    if let Ok(mut entry) = archive.by_name("media_map.json") {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        if let Ok(map) = serde_json::from_str::<Vec<MediaEntry>>(&json) {
            result.media_map = map;
        }
    }

    if let Ok(mut entry) = archive.by_name("items.json") {
        let mut json = String::new();
        entry.read_to_string(&mut json).map_err(|e| e.to_string())?;
        let mut items: Vec<crate::database::ClipboardItem> =
            serde_json::from_str(&json).map_err(|e| format!("解析条目失败: {e}"))?;

        let media_index = build_media_index(&result.media_map);
        for item in &mut items {
            rewrite_item_media_paths(item, &media_index, data_dir);
        }

        let repo = crate::database::ClipboardRepository::new(db);
        let imported = repo
            .import_sync_items(&items)
            .map_err(|e| format!("导入条目失败: {e}"))?;
        result.items_imported = imported;
        info!("同步导入: {} 条记录", imported);
    }

    Ok(result)
}

/// 按来源路径索引媒体映射表（同一路径多设备条目取第一条）
pub fn build_media_index(map: &[MediaEntry]) -> std::collections::HashMap<&str, &MediaEntry> {
    let mut index = std::collections::HashMap::new();
    for entry in map {
        index.entry(entry.local_path.as_str()).or_insert(entry);
    }
    index
}

/// 将条目中指向失效路径的媒体引用改写为本机落地路径。
/// 返回是否发生改写。
pub fn rewrite_item_media_paths(
    item: &mut crate::database::ClipboardItem,
    media_index: &std::collections::HashMap<&str, &MediaEntry>,
    data_dir: &Path,
) -> bool {
    let mut changed = false;

    // 图片条目：image_path 失效时指向本机 hash 落地路径
    if item.content_type == "image"
        && let Some(ref p) = item.image_path
        && !Path::new(p).exists()
        && let Some(entry) = media_index.get(p.as_str())
        && let Some(target) = local_media_target(entry, data_dir)
    {
        item.image_path = Some(target.to_string_lossy().to_string());
        changed = true;
    }

    // 来源应用图标：所有类型的条目都可能携带
    if let Some(ref icon) = item.source_app_icon
        && !Path::new(icon).exists()
        && let Some(entry) = media_index.get(icon.as_str())
        && let Some(target) = local_media_target(entry, data_dir)
    {
        item.source_app_icon = Some(target.to_string_lossy().to_string());
        changed = true;
    }

    // 文件条目：原始路径失效时把 payload.staged 指向本机落地路径，
    // 之后 resolve_paths 的 original→staged 回退机制自然接管粘贴/有效性检查/另存为
    if item.content_type == "files"
        && let Some(ref paths_json) = item.file_paths
    {
        let paths: Vec<String> = serde_json::from_str(paths_json).unwrap_or_default();
        let mut payload =
            crate::clipboard::file_clipboard::decode_payload(item.file_payload.as_deref())
                .unwrap_or_default();
        let mut payload_changed = false;

        for path in &paths {
            if Path::new(path).exists() {
                continue;
            }
            let Some(entry) = media_index.get(path.as_str()) else {
                continue;
            };
            let Some(target) = local_media_target(entry, data_dir) else {
                continue;
            };
            let target_str = target.to_string_lossy().to_string();
            match payload.staged.iter_mut().find(|s| &s.original == path) {
                Some(staged) => {
                    if staged.staged != target_str && !Path::new(&staged.staged).exists() {
                        staged.staged = target_str;
                        payload_changed = true;
                    }
                }
                None => {
                    payload
                        .staged
                        .push(crate::clipboard::file_clipboard::StagedFile {
                            original: path.clone(),
                            staged: target_str,
                            size: entry.size,
                        });
                    payload_changed = true;
                }
            }
        }

        if payload_changed {
            item.file_payload = Some(crate::clipboard::file_clipboard::encode_payload(&payload));
            changed = true;
        }
    }

    changed
}

/// 修复库中指向失效路径的媒体记录（跨设备遗留死记录自愈）。
/// 返回修复的条目数。
pub fn reconcile_local_media(
    db: &crate::database::Database,
    media_map: &[MediaEntry],
    data_dir: &Path,
) -> usize {
    if media_map.is_empty() {
        return 0;
    }
    let repo = crate::database::ClipboardRepository::new(db);
    let Ok(items) = repo.query_media_items() else {
        return 0;
    };
    let media_index = build_media_index(media_map);

    let mut fixed = 0usize;
    for mut item in items {
        if rewrite_item_media_paths(&mut item, &media_index, data_dir) {
            match repo.update_item_media_paths(
                item.id,
                item.image_path.as_deref(),
                item.file_payload.as_deref(),
                item.source_app_icon.as_deref(),
            ) {
                Ok(()) => fixed += 1,
                Err(e) => warn!("修复条目 {} 媒体路径失败: {}", item.id, e),
            }
        }
    }
    if fixed > 0 {
        info!("媒体路径自愈: 修复 {} 条记录", fixed);
    }
    fixed
}

/// 计算需要从云端下载的媒体条目：本机落地文件缺失且被库中条目引用。
pub fn plan_media_downloads(
    db: &crate::database::Database,
    media_map: &[MediaEntry],
    data_dir: &Path,
) -> Vec<MediaEntry> {
    if media_map.is_empty() {
        return Vec::new();
    }
    let repo = crate::database::ClipboardRepository::new(db);
    let Ok(items) = repo.query_media_items() else {
        return Vec::new();
    };

    // 收集库中所有被引用的媒体路径（图片、图标、文件原始路径与 staged 路径）
    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &items {
        if let Some(ref p) = item.image_path {
            referenced.insert(p.clone());
        }
        if let Some(ref p) = item.source_app_icon {
            referenced.insert(p.clone());
        }
        if let Some(ref paths_json) = item.file_paths
            && let Ok(paths) = serde_json::from_str::<Vec<String>>(paths_json)
        {
            referenced.extend(paths);
        }
        referenced.extend(crate::clipboard::file_clipboard::staged_paths_from_payload(
            item.file_payload.as_deref(),
        ));
    }

    media_map
        .iter()
        .filter(|entry| {
            let Some(target) = local_media_target(entry, data_dir) else {
                return false;
            };
            if target.exists() {
                return false;
            }
            // 本机落地路径或来源路径被引用均视为需要
            referenced.contains(target.to_string_lossy().as_ref())
                || referenced.contains(&entry.local_path)
        })
        .cloned()
        .collect()
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
    if let Some(data) = download_sync(config, "media_map.json")? {
        let json = String::from_utf8(data).map_err(|e| format!("解析 UTF-8 失败: {e}"))?;
        let map: Vec<MediaEntry> =
            serde_json::from_str(&json).map_err(|e| format!("解析 media_map.json 失败: {e}"))?;
        info!("下载 media_map.json: {} 条", map.len());
        Ok(map)
    } else {
        info!("远端无 media_map.json");
        Ok(Vec::new())
    }
}

/// 上传 media_map.json（多设备安全合并）
pub fn upload_media_map(
    config: &WebDavConfig,
    local_entries: &[MediaEntry],
    device_id: &str,
) -> Result<Vec<MediaEntry>, String> {
    let mut map = download_media_map(config).unwrap_or_default();

    let local_hashes: std::collections::HashSet<&str> =
        local_entries.iter().map(|e| e.hash.as_str()).collect();
    let before = map.len();
    map.retain(|e| {
        if e.device_id == device_id || e.device_id.is_empty() {
            local_hashes.contains(e.hash.as_str())
        } else {
            true
        }
    });
    let removed = before - map.len();

    let existing: std::collections::HashSet<(String, String)> = map
        .iter()
        .map(|e| (e.hash.clone(), e.device_id.clone()))
        .collect();
    let mut added = 0usize;
    for entry in local_entries {
        if !existing.contains(&(entry.hash.clone(), entry.device_id.clone())) {
            map.push(entry.clone());
            added += 1;
        }
    }

    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    upload_sync(
        config,
        json.as_bytes(),
        "media_map.json",
        "application/json",
    )?;
    if added > 0 || removed > 0 {
        info!(
            "上传 media_map.json: {} 条 (新增 {}, 移除 {})",
            map.len(),
            added,
            removed
        );
    }
    Ok(map)
}

/// 通过 PROPFIND 列出远端 media/ 目录下的文件名
fn list_remote_media_files(config: &WebDavConfig) -> Result<Vec<String>, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let media_url = format!("{base_url}media/");

    let resp = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap(),
            &media_url,
        )
        .header("Authorization", &auth)
        .header("Depth", "1")
        .send()
        .map_err(|e| format!("PROPFIND media/ 失败: {e}"))?;

    if !resp.status().is_success() {
        return Ok(Vec::new());
    }

    let body = resp
        .text()
        .map_err(|e| format!("读取 PROPFIND 响应失败: {e}"))?;
    let lower = body.to_lowercase();

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
                if let Some(name) = decoded.trim_end_matches('/').rsplit('/').next()
                    && name.contains('.')
                {
                    files.push(name.to_string());
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
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(val) = u8::from_str_radix(&input[i + 1..i + 3], 16)
        {
            result.push(val);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

/// 清理云端不再被任何设备引用的媒体文件
pub fn cleanup_orphaned_remote_media(
    config: &WebDavConfig,
    merged_map: &[MediaEntry],
) -> Result<usize, String> {
    let remote_files = list_remote_media_files(config)?;
    if remote_files.is_empty() {
        return Ok(0);
    }

    let referenced_hashes: std::collections::HashSet<&str> =
        merged_map.iter().map(|e| e.hash.as_str()).collect();

    fn remote_hash(filename: &str) -> Option<&str> {
        filename.rsplit_once('.').map(|(hash, _)| hash)
    }
    let orphan_files: Vec<&String> = remote_files
        .iter()
        .filter(|f| {
            if let Some(hash) = remote_hash(f) {
                !referenced_hashes.contains(hash)
            } else {
                false
            }
        })
        .collect();

    if orphan_files.is_empty() {
        return Ok(0);
    }

    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let mut deleted = 0usize;

    for filename in &orphan_files {
        let remote_url = format!("{base_url}media/{filename}");
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

    let orphan_hashes: std::collections::HashSet<&str> =
        orphan_files.iter().filter_map(|f| remote_hash(f)).collect();
    if !orphan_hashes.is_empty()
        && let Ok(mut map) = download_media_map(config)
    {
        let before = map.len();
        map.retain(|e| !orphan_hashes.contains(e.hash.as_str()));
        if map.len() < before {
            let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
            let _ = upload_sync(
                config,
                json.as_bytes(),
                "media_map.json",
                "application/json",
            );
        }
    }

    info!("云端媒体清理完成: 删除 {} 个", deleted);
    Ok(deleted)
}

/// 上传数据到 WebDAV
pub fn upload_sync(
    config: &WebDavConfig,
    data: &[u8],
    filename: &str,
    content_type: &str,
) -> Result<(), String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);

    ensure_remote_dir(&client, &config.url, &config.remote_dir, &auth)?;

    let file_url = format!("{base_url}{filename}");
    info!("WebDAV 上传: {} ({} bytes)", file_url, data.len());

    let resp = client
        .put(&file_url)
        .header("Authorization", &auth)
        .header("Content-Type", content_type)
        .body(data.to_vec())
        .send()
        .map_err(|e| format!("上传失败: {e}"))?;

    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        info!("WebDAV 上传成功: HTTP {}", status);
        Ok(())
    } else {
        Err(format!("上传失败: HTTP {status}"))
    }
}

/// 从数据库加载 WebDAV 配置和同步选项
pub(crate) fn load_config_and_options(
    db: &crate::database::Database,
) -> Option<(WebDavConfig, SyncOptions)> {
    let repo = crate::database::SettingsRepository::new(db);
    let url = repo.get("webdav_url").ok().flatten().unwrap_or_default();
    if url.is_empty() {
        return None;
    }
    let username = repo
        .get("webdav_username")
        .ok()
        .flatten()
        .unwrap_or_default();
    let password = repo
        .get("webdav_password")
        .ok()
        .flatten()
        .unwrap_or_default();
    let remote_dir = repo
        .get("webdav_remote_dir")
        .ok()
        .flatten()
        .unwrap_or_else(|| "/elegant-clipboard".to_string());

    let get_bool = |key: &str, default: bool| -> bool {
        repo.get(key)
            .ok()
            .flatten()
            .map_or(default, |v| v != "false")
    };
    let get_u64 = |key: &str, default: u64| -> u64 {
        repo.get(key)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let options = SyncOptions {
        sync_text: get_bool("webdav_sync_text", true),
        sync_image: get_bool("webdav_sync_image", true),
        sync_files: get_bool("webdav_sync_files", true),
        sync_video: false,
        sync_settings: true,
        max_image_size_kb: get_u64("webdav_max_image_size_kb", 5120),
        max_file_size_kb: get_u64("webdav_max_file_size_kb", 5120),
        max_video_size_kb: get_u64("webdav_max_video_size_kb", 5120),
    };

    let proxy_mode = repo
        .get("webdav_proxy_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| "system".to_string());
    let proxy_url = repo
        .get("webdav_proxy_url")
        .ok()
        .flatten()
        .unwrap_or_default();
    let accept_invalid_certs = get_bool("webdav_accept_invalid_certs", false);

    Some((
        WebDavConfig {
            url,
            username,
            password,
            remote_dir,
            proxy_mode,
            proxy_url,
            accept_invalid_certs,
        },
        options,
    ))
}

/// 防止 ZIP 记录同步并发（手动上传/下载与自动同步互斥）
static SYNC_SESSION_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 持有期间独占 WebDAV 记录同步会话；Drop 时自动释放。
pub struct SyncSessionGuard;

impl Drop for SyncSessionGuard {
    fn drop(&mut self) {
        SYNC_SESSION_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 后台媒体线程持有，延长 SyncSessionGuard 生命周期直至全部完成
struct SyncSessionHolder {
    _guard: SyncSessionGuard,
}

fn finish_auto_media_worker(
    pending: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    _session: Option<std::sync::Arc<SyncSessionHolder>>,
) {
    if pending.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
        MEDIA_SYNC_RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 写入最近同步时间（精确到秒）
pub fn record_last_sync_time(db: &crate::database::Database) -> Result<String, String> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    crate::database::SettingsRepository::new(db)
        .set("webdav_last_sync_time", &now)
        .map_err(|e| format!("写入同步时间失败: {e}"))?;
    Ok(now)
}

/// 通知前端刷新「上次同步」显示
pub fn emit_last_sync_updated(app: &tauri::AppHandle, time: &str) -> Result<(), String> {
    use tauri::Emitter;
    app.emit("webdav-last-sync-updated", time.to_string())
        .map_err(|e| format!("推送同步时间事件失败: {e}"))
}

/// 写入并通知最近同步时间
pub fn record_and_notify_last_sync(
    db: &crate::database::Database,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let time = record_last_sync_time(db)?;
    emit_last_sync_updated(app, &time)?;
    Ok(time)
}

/// 尝试开始同步会话；已有会话进行中时返回错误。
pub fn try_begin_sync_session() -> Result<SyncSessionGuard, String> {
    if SYNC_SESSION_ACTIVE
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return Err(SYNC_SESSION_BUSY.to_string());
    }
    Ok(SyncSessionGuard)
}

/// 用于追踪媒体同步是否正在进行
static MEDIA_SYNC_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 防止重复启动自动同步后台线程
static AUTO_SYNC_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 启动后台自动同步任务（仅在插件启用时调用）
pub fn start_auto_sync_task(
    db: crate::database::Database,
    data_dir: std::path::PathBuf,
    app: tauri::AppHandle,
) {
    if AUTO_SYNC_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("webdav-auto-sync".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));

            let mut cycle_count: u64 = 0;

            loop {
                let settings_repo = crate::database::SettingsRepository::new(&db);
                let plugin_enabled = settings_repo.get_bool("plugin_webdav_enabled", false);
                let enabled = settings_repo
                    .get("webdav_enabled")
                    .ok()
                    .flatten()
                    .is_some_and(|v| v == "true");
                let auto_sync = settings_repo
                    .get("webdav_auto_sync")
                    .ok()
                    .flatten()
                    .is_some_and(|v| v == "true");

                if plugin_enabled && enabled && auto_sync {
                    let interval_secs: u64 = settings_repo
                        .get("webdav_sync_interval")
                        .ok()
                        .flatten()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(60);

                    if let Some((config, options)) = load_config_and_options(&db) {
                        match try_begin_sync_session() {
                            Ok(guard) => {
                                info!("WebDAV 轻量同步: 开始上传");
                                match export_sync_data(&db, &data_dir, &options) {
                                    Ok(zip_data) => {
                                        if let Err(e) = upload_sync(
                                            &config,
                                            &zip_data,
                                            SYNC_FILENAME,
                                            "application/zip",
                                        ) {
                                            info!("WebDAV 轻量同步上传失败: {}", e);
                                        } else {
                                            match record_and_notify_last_sync(&db, &app) {
                                                Ok(_) => info!("WebDAV 轻量同步: 上传完成"),
                                                Err(e) => {
                                                    warn!(
                                                        "WebDAV 轻量同步: 记录同步时间失败: {}",
                                                        e
                                                    )
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => info!("WebDAV 轻量同步导出失败: {}", e),
                                }

                                let need_media = (options.sync_image || options.sync_files)
                                    && cycle_count.is_multiple_of(3);
                                if need_media
                                    && !MEDIA_SYNC_RUNNING
                                        .load(std::sync::atomic::Ordering::Relaxed)
                                {
                                    MEDIA_SYNC_RUNNING
                                        .store(true, std::sync::atomic::Ordering::Relaxed);

                                    let device_id = get_or_create_device_id(&db);
                                    let local_items =
                                        query_sync_items(&db, &options).unwrap_or_default();
                                    let local_map = build_media_map(
                                        &local_items,
                                        &data_dir,
                                        &options,
                                        &device_id,
                                    );

                                    let merged_map = if local_map.is_empty() {
                                        let map = download_media_map(&config).unwrap_or_default();
                                        let _ = cleanup_orphaned_remote_media(&config, &map);
                                        map
                                    } else {
                                        match upload_media_map(&config, &local_map, &device_id) {
                                            Ok(map) => {
                                                let _ =
                                                    cleanup_orphaned_remote_media(&config, &map);
                                                map
                                            }
                                            Err(e) => {
                                                info!("上传 media map 失败，跳过清理: {}", e);
                                                Vec::new()
                                            }
                                        }
                                    };

                                    let local_images: Vec<MediaEntry> = local_map
                                        .iter()
                                        .filter(|e| e.media_type == "image")
                                        .cloned()
                                        .collect();
                                    let local_files: Vec<MediaEntry> = local_map
                                        .iter()
                                        .filter(|e| e.media_type == "file")
                                        .cloned()
                                        .collect();
                                    let local_icons: Vec<MediaEntry> = local_map
                                        .iter()
                                        .filter(|e| e.media_type == "icon")
                                        .cloned()
                                        .collect();

                                    let _ = reconcile_local_media(&db, &merged_map, &data_dir);
                                    let needed = plan_media_downloads(&db, &merged_map, &data_dir);
                                    let dl_images: Vec<MediaEntry> = needed
                                        .iter()
                                        .filter(|e| e.media_type == "image")
                                        .cloned()
                                        .collect();
                                    let dl_icons: Vec<MediaEntry> = needed
                                        .iter()
                                        .filter(|e| e.media_type == "icon")
                                        .cloned()
                                        .collect();
                                    let dl_files: Vec<MediaEntry> = needed
                                        .into_iter()
                                        .filter(|e| e.media_type == "file")
                                        .collect();

                                    let pending =
                                        std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

                                    if options.sync_image
                                        && (!local_images.is_empty() || !dl_images.is_empty())
                                    {
                                        pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }
                                    if options.sync_files
                                        && (!local_files.is_empty() || !dl_files.is_empty())
                                    {
                                        pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }
                                    if !local_icons.is_empty() || !dl_icons.is_empty() {
                                        pending.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }

                                    let session =
                                        if pending.load(std::sync::atomic::Ordering::Relaxed) > 0 {
                                            Some(std::sync::Arc::new(SyncSessionHolder {
                                                _guard: guard,
                                            }))
                                        } else {
                                            MEDIA_SYNC_RUNNING
                                                .store(false, std::sync::atomic::Ordering::Relaxed);
                                            None
                                        };

                                    if let Some(session) = session {
                                        if options.sync_image
                                            && (!local_images.is_empty() || !dl_images.is_empty())
                                        {
                                            let cfg = config.clone();
                                            let dir = data_dir.clone();
                                            let cnt = pending.clone();
                                            let sess = session.clone();
                                            match std::thread::Builder::new()
                                                .name("webdav-sync-images".into())
                                                .spawn(move || {
                                                    if !local_images.is_empty() {
                                                        match upload_media_files(
                                                            &cfg,
                                                            &local_images,
                                                            &dir,
                                                        ) {
                                                            Ok((u, s, _)) => {
                                                                info!(
                                                                    "图片上传: {} 新, {} 跳过",
                                                                    u, s
                                                                );
                                                            }
                                                            Err(e) => {
                                                                info!("图片上传失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    if !dl_images.is_empty() {
                                                        match download_missing_media(
                                                            &cfg, &dl_images, &dir,
                                                        ) {
                                                            Ok(n) if n > 0 => {
                                                                info!("图片下载: {} 个", n)
                                                            }
                                                            Ok(_) => {}
                                                            Err(e) => {
                                                                info!("图片下载失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    finish_auto_media_worker(&cnt, Some(sess));
                                                }) {
                                                Ok(_) => {}
                                                Err(e) => {
                                                    info!("图片同步线程创建失败: {}", e);
                                                    finish_auto_media_worker(
                                                        &pending,
                                                        Some(session.clone()),
                                                    );
                                                }
                                            }
                                        }

                                        if options.sync_files
                                            && (!local_files.is_empty() || !dl_files.is_empty())
                                        {
                                            let cfg = config.clone();
                                            let dir = data_dir.clone();
                                            let cnt = pending.clone();
                                            let sess = session.clone();
                                            match std::thread::Builder::new()
                                                .name("webdav-sync-files".into())
                                                .spawn(move || {
                                                    if !local_files.is_empty() {
                                                        match upload_media_files(
                                                            &cfg,
                                                            &local_files,
                                                            &dir,
                                                        ) {
                                                            Ok((u, s, _)) => {
                                                                info!(
                                                                    "文件上传: {} 新, {} 跳过",
                                                                    u, s
                                                                );
                                                            }
                                                            Err(e) => {
                                                                info!("文件上传失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    if !dl_files.is_empty() {
                                                        match download_missing_media(
                                                            &cfg, &dl_files, &dir,
                                                        ) {
                                                            Ok(n) if n > 0 => {
                                                                info!("文件下载: {} 个", n)
                                                            }
                                                            Ok(_) => {}
                                                            Err(e) => {
                                                                info!("文件下载失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    finish_auto_media_worker(&cnt, Some(sess));
                                                }) {
                                                Ok(_) => {}
                                                Err(e) => {
                                                    info!("文件同步线程创建失败: {}", e);
                                                    finish_auto_media_worker(
                                                        &pending,
                                                        Some(session.clone()),
                                                    );
                                                }
                                            }
                                        }

                                        if !local_icons.is_empty() || !dl_icons.is_empty() {
                                            let cfg = config.clone();
                                            let dir = data_dir.clone();
                                            let cnt = pending.clone();
                                            let sess = session.clone();
                                            match std::thread::Builder::new()
                                                .name("webdav-sync-icons".into())
                                                .spawn(move || {
                                                    if !local_icons.is_empty() {
                                                        match upload_media_files(
                                                            &cfg,
                                                            &local_icons,
                                                            &dir,
                                                        ) {
                                                            Ok((u, s, _)) => {
                                                                info!(
                                                                    "图标上传: {} 新, {} 跳过",
                                                                    u, s
                                                                );
                                                            }
                                                            Err(e) => {
                                                                info!("图标上传失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    if !dl_icons.is_empty() {
                                                        match download_missing_media(
                                                            &cfg, &dl_icons, &dir,
                                                        ) {
                                                            Ok(n) if n > 0 => {
                                                                info!("图标下载: {} 个", n)
                                                            }
                                                            Ok(_) => {}
                                                            Err(e) => {
                                                                info!("图标下载失败: {}", e)
                                                            }
                                                        }
                                                    }
                                                    finish_auto_media_worker(&cnt, Some(sess));
                                                }) {
                                                Ok(_) => {}
                                                Err(e) => {
                                                    info!("图标同步线程创建失败: {}", e);
                                                    finish_auto_media_worker(
                                                        &pending,
                                                        Some(session.clone()),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => info!("WebDAV 轻量同步: 跳过（{}）", e),
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
    {
        tracing::error!("Failed to spawn webdav-auto-sync thread: {e}");
    }
}

/// 从 WebDAV 下载 ZIP
pub fn download_sync(config: &WebDavConfig, filename: &str) -> Result<Option<Vec<u8>>, String> {
    let client = build_client(config)?;
    let auth = basic_auth(&config.username, &config.password);
    let base_url = normalize_url(&config.url, &config.remote_dir);
    let file_url = format!("{base_url}{filename}");

    info!("WebDAV 下载: {}", file_url);

    let resp = client
        .get(&file_url)
        .header("Authorization", &auth)
        .send()
        .map_err(|e| format!("下载失败: {e}"))?;

    let status = resp.status().as_u16();
    match status {
        200..=299 => {
            if let Some(len) = resp.content_length()
                && len > MAX_SYNC_DOWNLOAD_BYTES
            {
                return Err(format!(
                    "同步文件过大: {len} bytes，超过 {MAX_SYNC_DOWNLOAD_BYTES} bytes 上限"
                ));
            }

            let mut limited = resp.take(MAX_SYNC_DOWNLOAD_BYTES + 1);
            let mut data = Vec::new();
            limited
                .read_to_end(&mut data)
                .map_err(|e| format!("读取响应失败: {e}"))?;
            if data.len() as u64 > MAX_SYNC_DOWNLOAD_BYTES {
                return Err(format!(
                    "同步文件过大: 超过 {MAX_SYNC_DOWNLOAD_BYTES} bytes 上限"
                ));
            }
            info!("WebDAV 下载成功: {} bytes", data.len());
            Ok(Some(data))
        }
        404 => {
            info!("远端无同步文件");
            Ok(None)
        }
        _ => Err(format!("下载失败: HTTP {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MediaEntry, SYNC_SESSION_ACTIVE, SYNC_SESSION_BUSY, SyncOptions, build_media_index,
        local_media_target, rewrite_item_media_paths, sync_query_limits, try_begin_sync_session,
    };
    use std::path::Path;

    fn default_options() -> SyncOptions {
        SyncOptions {
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

    fn media_entry(media_type: &str, local_path: &str, hash: &str) -> MediaEntry {
        MediaEntry {
            hash: hash.to_string(),
            ext: "png".to_string(),
            media_type: media_type.to_string(),
            local_path: local_path.to_string(),
            device_id: "dev-a".to_string(),
            file_name: String::new(),
            size: 100,
            source_path: None,
        }
    }

    #[test]
    fn query_limits_all_enabled() {
        let (text, image, files) = sync_query_limits(&default_options());
        assert!(text);
        assert_eq!(image, Some(5120 * 1024));
        assert_eq!(files, Some(5120 * 1024));
    }

    #[test]
    fn query_limits_disabled_types_are_none() {
        let opts = SyncOptions {
            sync_text: false,
            sync_image: false,
            sync_files: false,
            ..default_options()
        };
        let (text, image, files) = sync_query_limits(&opts);
        assert!(!text);
        assert_eq!(image, None);
        assert_eq!(files, None);
    }

    #[test]
    fn query_limits_zero_means_unlimited() {
        let opts = SyncOptions {
            max_image_size_kb: 0,
            ..default_options()
        };
        let (_, image, _) = sync_query_limits(&opts);
        assert_eq!(image, Some(i64::MAX));
    }

    #[test]
    fn media_target_image_uses_hash_in_local_images_dir() {
        let entry = media_entry("image", "C:\\Users\\A\\data\\images\\old.png", "abc123");
        let target = local_media_target(&entry, Path::new("D:\\data")).unwrap();
        assert_eq!(
            target,
            Path::new("D:\\data").join("images").join("abc123.png")
        );
    }

    #[test]
    fn media_target_icon_keeps_cache_filename() {
        let entry = media_entry("icon", "C:\\Users\\A\\data\\icons\\key123.png", "h1");
        let target = local_media_target(&entry, Path::new("D:\\data")).unwrap();
        assert_eq!(
            target,
            Path::new("D:\\data").join("icons").join("key123.png")
        );
    }

    #[test]
    fn media_target_file_goes_to_staged_webdav() {
        let mut entry = media_entry("file", "D:\\Downloads\\report.zip", "aabbccddeeff00112233");
        entry.file_name = "report.zip".to_string();
        let target = local_media_target(&entry, Path::new("E:\\app")).unwrap();
        assert_eq!(
            target,
            Path::new("E:\\app")
                .join("staged")
                .join("webdav")
                .join("aabbccddeeff0011_report.zip")
        );
    }

    #[test]
    fn media_target_file_falls_back_to_local_path_name() {
        let entry = media_entry("file", "D:\\Downloads\\notes.txt", "hash1234");
        let target = local_media_target(&entry, Path::new("E:\\app")).unwrap();
        assert!(target.to_string_lossy().ends_with("hash1234_notes.txt"));
    }

    #[test]
    fn media_target_rejects_empty_hash() {
        let entry = media_entry("image", "a.png", "");
        assert!(local_media_target(&entry, Path::new("D:\\data")).is_none());
    }

    #[test]
    fn media_entry_deserializes_legacy_json_without_new_fields() {
        let json = r#"{"hash":"h1","ext":"png","media_type":"image","local_path":"a.png","device_id":"d1"}"#;
        let entry: MediaEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.hash, "h1");
        assert_eq!(entry.file_name, "");
        assert_eq!(entry.size, 0);
        assert!(entry.source_path.is_none());
    }

    #[test]
    fn media_entry_serializes_without_source_path() {
        let mut entry = media_entry("image", "a.png", "h1");
        entry.source_path = Some("C:\\local\\a.png".to_string());
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("source_path"));
    }

    fn test_item(content_type: &str) -> crate::database::ClipboardItem {
        crate::database::ClipboardItem {
            id: 1,
            content_type: content_type.to_string(),
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: None,
            file_payload: None,
            content_hash: "ch".into(),
            semantic_hash: "sh".into(),
            preview: None,
            byte_size: 0,
            image_width: None,
            image_height: None,
            is_pinned: false,
            is_favorite: false,
            favorite_order: 0,
            sort_order: 0,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
            access_count: 0,
            last_accessed_at: None,
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
            group_id: None,
            files_valid: None,
        }
    }

    #[test]
    fn rewrite_image_path_to_local_target() {
        let map = vec![media_entry(
            "image",
            "C:\\other-device\\images\\x.png",
            "hash1",
        )];
        let index = build_media_index(&map);
        let mut item = test_item("image");
        item.image_path = Some("C:\\other-device\\images\\x.png".to_string());

        let changed = rewrite_item_media_paths(&mut item, &index, Path::new("D:\\data"));
        assert!(changed);
        assert_eq!(
            item.image_path.as_deref().map(std::path::PathBuf::from),
            Some(Path::new("D:\\data").join("images").join("hash1.png"))
        );
    }

    #[test]
    fn rewrite_files_payload_adds_staged_entry() {
        let mut entry = media_entry("file", "C:\\other-device\\doc.pdf", "hash2");
        entry.ext = "pdf".into();
        entry.file_name = "doc.pdf".into();
        entry.size = 42;
        let map = vec![entry];
        let index = build_media_index(&map);

        let mut item = test_item("files");
        item.file_paths = Some(r#"["C:\\other-device\\doc.pdf"]"#.to_string());

        let changed = rewrite_item_media_paths(&mut item, &index, Path::new("D:\\data"));
        assert!(changed);

        let payload =
            crate::clipboard::file_clipboard::decode_payload(item.file_payload.as_deref()).unwrap();
        assert_eq!(payload.staged.len(), 1);
        assert_eq!(payload.staged[0].original, "C:\\other-device\\doc.pdf");
        assert_eq!(payload.staged[0].size, 42);
        assert!(payload.staged[0].staged.contains("staged"));
        assert!(payload.staged[0].staged.ends_with("hash2_doc.pdf"));
    }

    #[test]
    fn rewrite_skips_unmapped_paths() {
        let index = build_media_index(&[]);
        let mut item = test_item("image");
        item.image_path = Some("C:\\other-device\\images\\x.png".to_string());

        let changed = rewrite_item_media_paths(&mut item, &index, Path::new("D:\\data"));
        assert!(!changed);
        assert_eq!(
            item.image_path.as_deref(),
            Some("C:\\other-device\\images\\x.png")
        );
    }

    #[test]
    fn sync_session_rejects_concurrent_begin() {
        SYNC_SESSION_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        let _first = try_begin_sync_session().unwrap();
        assert_eq!(
            try_begin_sync_session().err().as_deref(),
            Some(SYNC_SESSION_BUSY),
        );
        SYNC_SESSION_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn media_target_file_rejects_traversal_in_file_name() {
        let mut entry = media_entry("file", "D:\\Downloads\\report.zip", "aabbccddeeff00112233");
        entry.file_name = "..\\..\\evil.exe".to_string();
        assert!(local_media_target(&entry, Path::new("E:\\app")).is_none());
    }
}
