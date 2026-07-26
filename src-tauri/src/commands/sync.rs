use crate::commands::AppState;
use crate::config;
use crate::database::SettingsRepository;
use crate::utils::format_size;
use crate::webdav::{self, SyncOptions};
use std::sync::Arc;
use tauri::State;

/// 手动同步命令的返回体
#[derive(serde::Serialize)]
pub struct WebdavManualSyncResponse {
    pub message: String,
    /// 后台媒体任务数（图片/文件/图标各算一个，仅非空列表）
    pub pending_media_workers: u8,
}

/// 从数据库加载 WebDAV 配置，URL 为空时返回错误
fn load_webdav_config(
    db: &crate::database::Database,
) -> Result<(webdav::WebDavConfig, webdav::SyncOptions), String> {
    webdav::load_config_and_options(db).ok_or_else(|| "WebDAV 地址未配置".to_string())
}

/// 获取数据目录
fn get_data_dir() -> std::path::PathBuf {
    config::AppConfig::load().get_data_dir()
}

/// 检查 WebDAV 插件已启用
fn ensure_webdav_plugin_enabled(state: &Arc<AppState>) -> Result<(), String> {
    let repo = SettingsRepository::new(&state.db);
    if !repo.get_bool("plugin_webdav_enabled", false) {
        return Err("WebDAV 插件未启用".to_string());
    }
    Ok(())
}

/// 检查 WebDAV 插件与同步开关均已启用
fn ensure_webdav_available(state: &Arc<AppState>) -> Result<(), String> {
    ensure_webdav_plugin_enabled(state)?;
    let repo = SettingsRepository::new(&state.db);
    if !repo.get_bool("webdav_enabled", false) {
        return Err("WebDAV 同步未开启".to_string());
    }
    Ok(())
}

/// 运行时启用 WebDAV 插件（启动自动同步后台任务）
#[tauri::command]
pub async fn webdav_enable_plugin(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    webdav::start_auto_sync_task(state.db.clone(), get_data_dir(), app);
    Ok(())
}
/// 测试 WebDAV 连接
#[tauri::command]
pub async fn webdav_test_connection(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    ensure_webdav_plugin_enabled(&state)?;
    let (config, _) = load_webdav_config(&state.db)?;
    tokio::task::spawn_blocking(move || webdav::test_connection(&config))
        .await
        .map_err(|e| format!("任务失败: {e}"))?
}

/// 上传同步（本地 → 远端）
#[tauri::command]
pub async fn webdav_upload(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<WebdavManualSyncResponse, String> {
    ensure_webdav_available(&state)?;
    let (config, options) = load_webdav_config(&state.db)?;
    let data_dir = get_data_dir();
    let db = state.db.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        let _guard = webdav::try_begin_sync_session()?;
        let zip_data = webdav::export_sync_data(&db, &data_dir, &options)?;
        let size = zip_data.len();
        webdav::upload_sync(&config, &zip_data, "clipboard_sync.zip", "application/zip")?;

        let device_id = webdav::get_or_create_device_id(&db);
        let local_map = build_local_media_map(&db, &data_dir, &options, &device_id);
        if local_map.is_empty() {
            let map = webdav::download_media_map(&config).unwrap_or_default();
            let _ = webdav::cleanup_orphaned_remote_media(&config, &map);
        } else {
            match webdav::upload_media_map(&config, &local_map, &device_id) {
                Ok(map) => {
                    let _ = webdav::cleanup_orphaned_remote_media(&config, &map);
                }
                Err(e) => {
                    tracing::warn!("上传 media map 失败，跳过清理: {}", e);
                }
            }
        }

        let pending_media_workers = spawn_media_upload_files(&app, &config, &data_dir, &local_map);

        webdav::record_and_notify_last_sync(&db, &app_handle)?;

        let mut msg = format!("记录已上传 ({})", format_size(size as u64));
        if pending_media_workers > 0 {
            msg.push_str("\n媒体文件正在后台上传…");
        }
        Ok(WebdavManualSyncResponse {
            message: msg,
            pending_media_workers,
        })
    })
    .await
    .map_err(|e| format!("任务失败: {e}"))?
}

/// 下载同步（远端 → 本地）
#[tauri::command]
pub async fn webdav_download(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<WebdavManualSyncResponse, String> {
    ensure_webdav_available(&state)?;
    let (config, options) = load_webdav_config(&state.db)?;
    let data_dir = get_data_dir();
    let db = state.db.clone();
    let app_handle = app.clone();

    tokio::task::spawn_blocking(move || {
        let _guard = webdav::try_begin_sync_session()?;
        let zip_data = webdav::download_sync(&config, "clipboard_sync.zip")?;
        let mut msg = match zip_data {
            Some(data) => {
                let result = webdav::import_sync_data(&db, &data, &options, &data_dir)?;
                let mut parts = Vec::new();
                if result.items_imported > 0 {
                    parts.push(format!("导入 {} 条记录", result.items_imported));
                }
                if result.settings_imported {
                    parts.push("设置已同步".to_string());
                }
                if parts.is_empty() {
                    "记录已下载，无新数据".to_string()
                } else {
                    format!("记录已下载：{}", parts.join("，"))
                }
            }
            None => "远端无同步数据".to_string(),
        };

        // 权威媒体映射表：先自愈库中失效的媒体路径，再按需下载缺失媒体
        let media_map = webdav::download_media_map(&config).unwrap_or_default();
        let mut pending_media_workers = 0u8;
        if !media_map.is_empty() {
            let fixed = webdav::reconcile_local_media(&db, &media_map, &data_dir);
            let needed = webdav::plan_media_downloads(&db, &media_map, &data_dir);
            if needed.is_empty() {
                tracing::debug!(
                    "media_map 含 {} 条目，当前无需下载（可能已落地或未引用）",
                    media_map.len()
                );
                if fixed > 0 {
                    webdav::emit_webdav_media_ready(&app);
                }
            } else {
                pending_media_workers =
                    spawn_media_download(&app, &config, &data_dir, &db, &media_map, needed);
            }
        }

        webdav::record_and_notify_last_sync(&db, &app_handle)?;

        if pending_media_workers > 0 {
            msg.push_str("\n媒体文件正在后台下载…");
        }

        Ok(WebdavManualSyncResponse {
            message: msg,
            pending_media_workers,
        })
    })
    .await
    .map_err(|e| format!("任务失败: {e}"))?
}

/// 从数据库构建本地媒体映射表
fn build_local_media_map(
    db: &crate::database::Database,
    data_dir: &std::path::Path,
    options: &SyncOptions,
    device_id: &str,
) -> Vec<webdav::MediaEntry> {
    let items = webdav::query_sync_items(db, options).unwrap_or_default();
    let (map, _) = webdav::build_media_map(&items, data_dir, options, device_id);
    map
}

struct MediaSyncComplete {
    db: crate::database::Database,
    data_dir: std::path::PathBuf,
    media_map: Vec<webdav::MediaEntry>,
    app: tauri::AppHandle,
}

fn spawn_media_upload_worker(
    app: &tauri::AppHandle,
    config: &webdav::WebDavConfig,
    data_dir: &std::path::Path,
    entries: Vec<webdav::MediaEntry>,
    thread_name: &'static str,
    label: &'static str,
) -> bool {
    if entries.is_empty() {
        return false;
    }
    let cfg = config.clone();
    let dir = data_dir.to_path_buf();
    let handle = app.clone();
    match std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let msg = match webdav::upload_media_files(&cfg, &entries, &dir) {
                Ok((u, s, bytes)) => format!(
                    "{}上传完成：{} 新 ({})，{} 已存在跳过",
                    label,
                    u,
                    format_size(bytes),
                    s
                ),
                Err(e) => format!("{label}上传失败: {e}"),
            };
            emit_media_sync_done(&handle, &msg);
        }) {
        Ok(_) => true,
        Err(e) => {
            emit_media_sync_done(app, &format!("{label}上传线程启动失败: {e}"));
            true
        }
    }
}

fn spawn_media_download_worker(
    app: &tauri::AppHandle,
    config: &webdav::WebDavConfig,
    data_dir: &std::path::Path,
    entries: Vec<webdav::MediaEntry>,
    thread_name: &'static str,
    label: &'static str,
    pending: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    on_complete: std::sync::Arc<MediaSyncComplete>,
) -> bool {
    if entries.is_empty() {
        return false;
    }
    let cfg = config.clone();
    let dir = data_dir.to_path_buf();
    let handle = app.clone();
    let pending_worker = pending.clone();
    let on_complete_worker = on_complete.clone();
    match std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let msg = match webdav::download_missing_media(&cfg, &entries, &dir) {
                Ok(n) if n > 0 => format!("{label}下载完成：{n} 个文件"),
                Ok(_) => format!("{label}已是最新"),
                Err(e) => format!("{label}下载失败: {e}"),
            };
            if pending_worker.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                let _ = webdav::reconcile_local_media(
                    &on_complete_worker.db,
                    &on_complete_worker.media_map,
                    &on_complete_worker.data_dir,
                );
                webdav::emit_webdav_media_ready(&on_complete_worker.app);
            }
            emit_media_sync_done(&handle, &msg);
        }) {
        Ok(_) => true,
        Err(e) => {
            if pending.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) == 1 {
                let _ = webdav::reconcile_local_media(
                    &on_complete.db,
                    &on_complete.media_map,
                    &on_complete.data_dir,
                );
                webdav::emit_webdav_media_ready(&on_complete.app);
            }
            emit_media_sync_done(app, &format!("{label}下载线程启动失败: {e}"));
            true
        }
    }
}

fn spawn_media_upload_files(
    app: &tauri::AppHandle,
    config: &webdav::WebDavConfig,
    data_dir: &std::path::Path,
    media_map: &[webdav::MediaEntry],
) -> u8 {
    if media_map.is_empty() {
        return 0;
    }
    let images: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "image")
        .cloned()
        .collect();
    let files: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "file")
        .cloned()
        .collect();
    let icons: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "icon")
        .cloned()
        .collect();

    let mut workers = 0u8;
    if spawn_media_upload_worker(
        app,
        config,
        data_dir,
        images,
        "webdav-upload-images",
        "图片",
    ) {
        workers += 1;
    }
    if spawn_media_upload_worker(app, config, data_dir, files, "webdav-upload-files", "文件") {
        workers += 1;
    }
    if spawn_media_upload_worker(app, config, data_dir, icons, "webdav-upload-icons", "图标") {
        workers += 1;
    }
    workers
}

fn spawn_media_download(
    app: &tauri::AppHandle,
    config: &webdav::WebDavConfig,
    data_dir: &std::path::Path,
    db: &crate::database::Database,
    full_media_map: &[webdav::MediaEntry],
    media_map: Vec<webdav::MediaEntry>,
) -> u8 {
    let images: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "image")
        .cloned()
        .collect();
    let files: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "file")
        .cloned()
        .collect();
    let icons: Vec<_> = media_map
        .iter()
        .filter(|e| e.media_type == "icon")
        .cloned()
        .collect();

    let mut batch = 0usize;
    if !images.is_empty() {
        batch += 1;
    }
    if !files.is_empty() {
        batch += 1;
    }
    if !icons.is_empty() {
        batch += 1;
    }
    if batch == 0 {
        return 0;
    }

    let pending = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(batch));
    let on_complete = std::sync::Arc::new(MediaSyncComplete {
        db: db.clone(),
        data_dir: data_dir.to_path_buf(),
        media_map: full_media_map.to_vec(),
        app: app.clone(),
    });

    let mut workers = 0u8;
    if spawn_media_download_worker(
        app,
        config,
        data_dir,
        images,
        "webdav-download-images",
        "图片",
        pending.clone(),
        on_complete.clone(),
    ) {
        workers += 1;
    }
    if spawn_media_download_worker(
        app,
        config,
        data_dir,
        files,
        "webdav-download-files",
        "文件",
        pending.clone(),
        on_complete.clone(),
    ) {
        workers += 1;
    }
    if spawn_media_download_worker(
        app,
        config,
        data_dir,
        icons,
        "webdav-download-icons",
        "图标",
        pending,
        on_complete,
    ) {
        workers += 1;
    }
    workers
}

fn emit_media_sync_done(app: &tauri::AppHandle, message: &str) {
    use tauri::Emitter;
    if let Err(e) = app.emit("media-sync-done", message.to_string()) {
        tracing::warn!("推送媒体同步完成事件失败: {}", e);
    }
}
