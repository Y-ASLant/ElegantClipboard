use crate::commands::AppState;
use crate::config::{self, AppConfig};
use crate::database;
use tauri::Manager;

/// 临时取消设置窗口的置顶状态并隐藏主窗口（如可见），以便系统文件对话框不被遮挡
fn demote_windows_for_dialog(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_always_on_top(false);
    }
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        }
    }
}

/// 恢复设置窗口的置顶状态（主窗口保持隐藏，不自动恢复）
fn restore_windows_after_dialog(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
    }
}

fn chrono_timestamp() -> String {
    chrono::Local::now().format("%Y%m%d_%H%M%S").to_string()
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 检测并应用待导入的 staging 数据库文件（clipboard.db.import → clipboard.db）。
fn sanitize_zip_relative_path(name: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path, PathBuf};

    let raw = Path::new(name);
    if raw.is_absolute() {
        return None;
    }

    let mut clean = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(seg) => clean.push(seg),
            Component::CurDir => {}
            // 拒绝根/前缀/父目录防止路径穿越
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => return None,
        }
    }

    if clean.as_os_str().is_empty() {
        return None;
    }

    Some(clean)
}

pub(crate) fn apply_pending_import(db_path: &std::path::Path) {
    use std::fs;

    let staging = db_path.with_extension("db.import");
    if !staging.exists() {
        return;
    }

    tracing::info!("Detected pending import: {:?}", staging);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let db_dir = match db_path.parent() {
        Some(d) => d,
        None => return,
    };

    let mut applied = false;

    for attempt in 1..=10 {
        let deleted = ["", "-wal", "-shm"].iter().all(|ext| {
            let f = db_dir.join(format!("clipboard.db{ext}"));
            !f.exists() || fs::remove_file(&f).is_ok()
        });

        if deleted && fs::rename(&staging, db_path).is_ok() {
            tracing::info!("Import staging applied (attempt {attempt})");
            applied = true;
            break;
        }

        if attempt < 10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    if !applied {
        tracing::error!("Rename failed after 10 attempts, trying copy fallback");
        if fs::copy(&staging, db_path).is_ok() {
            let _ = fs::remove_file(&staging);
            tracing::info!("Import applied via copy fallback");
            applied = true;
        } else {
            tracing::error!("Import staging completely failed");
        }
    }

    // 导入的数据库可能来自其他设备，清除 device_id 以便重新生成
    if applied {
        if let Ok(conn) = rusqlite::Connection::open(db_path) {
            let _ = conn.execute("DELETE FROM settings WHERE key = 'device_id'", []);
            tracing::info!("Cleared device_id from imported database");
        }
    }
}

#[tauri::command]
pub fn get_default_data_path() -> String {
    let config = AppConfig::load();
    config.get_data_dir().to_string_lossy().to_string()
}

#[tauri::command]
pub fn get_original_default_path() -> String {
    database::get_default_db_path()
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub fn check_path_has_data(path: String) -> bool {
    let p = std::path::PathBuf::from(&path);
    p.join("clipboard.db").exists()
}

#[tauri::command]
pub fn cleanup_data_at_path(path: String) -> Result<(), String> {
    use std::fs;
    let p = std::path::PathBuf::from(&path);

    for ext in &["", "-wal", "-shm"] {
        let db_file = p.join(format!("clipboard.db{}", ext));
        if db_file.exists() {
            fs::remove_file(&db_file).map_err(|e| format!("删除 {:?} 失败: {}", db_file, e))?;
        }
    }

    let images_dir = p.join("images");
    if images_dir.exists() {
        fs::remove_dir_all(&images_dir)
            .map_err(|e| format!("删除图片目录失败: {}", e))?;
    }

    let icons_dir = p.join("icons");
    if icons_dir.exists() {
        fs::remove_dir_all(&icons_dir)
            .map_err(|e| format!("删除图标目录失败: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn set_data_path(path: String) -> Result<(), String> {
    let mut config = AppConfig::load();
    config.data_path = if path.is_empty() { None } else { Some(path) };
    config.save()
}

#[tauri::command]
pub fn migrate_data_to_path(new_path: String) -> Result<config::MigrationResult, String> {
    let config = AppConfig::load();
    let old_path = config.get_data_dir();
    let new_path = std::path::PathBuf::from(&new_path);

    if old_path == new_path {
        return Err("Source and destination paths are the same".to_string());
    }

    let result = config::migrate_data(&old_path, &new_path)?;

    if result.success() {
        let mut new_config = AppConfig::load();
        new_config.data_path = Some(new_path.to_string_lossy().to_string());
        new_config.save()?;
    }

    Ok(result)
}

#[tauri::command]
pub async fn export_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    let export_db = data_dir.join("clipboard.db.export");
    {
        let src_conn = state.db.write_connection();
        let src_conn = src_conn.lock();
        let _ = fs::remove_file(&export_db);
        let mut dst_conn = rusqlite::Connection::open(&export_db)
            .map_err(|e| format!("创建备份文件失败: {}", e))?;
        let backup = rusqlite::backup::Backup::new(&src_conn, &mut dst_conn)
            .map_err(|e| format!("初始化备份失败: {}", e))?;
        backup
            .run_to_completion(100, std::time::Duration::from_millis(0), None)
            .map_err(|e| format!("执行备份失败: {}", e))?;
    }

    let timestamp = chrono_timestamp();
    let default_name = format!("ElegantClipboard_backup_{}.zip", timestamp);
    demote_windows_for_dialog(&app);
    let dest = app
        .dialog()
        .file()
        .set_title("导出数据")
        .set_file_name(&default_name)
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_save_file();
    restore_windows_after_dialog(&app);

    let dest_path = match dest {
        Some(p) => p.to_string(),
        None => {
            let _ = fs::remove_file(&export_db);
            return Err("用户取消了导出".to_string());
        }
    };

    let file = File::create(&dest_path).map_err(|e| format!("创建文件失败: {}", e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("clipboard.db", options).map_err(|e| e.to_string())?;
    zip.write_all(&fs::read(&export_db).map_err(|e| format!("读取数据库副本失败: {}", e))?)
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&export_db);

    // 仅导出数据库引用的图片和图标文件（跳过孤立的磁盘文件）
    let repo = database::ClipboardRepository::new(&state.db);
    {
        let referenced_images = repo.get_all_image_paths().unwrap_or_default();
        let referenced_icons = repo.get_all_icon_paths().unwrap_or_default();
        for image_path in &referenced_images {
            let p = std::path::Path::new(image_path);
            if p.is_file() {
                if let Some(filename) = p.file_name() {
                    let zip_name = format!("images/{}", filename.to_string_lossy());
                    zip.start_file(&zip_name, options).map_err(|e| e.to_string())?;
                    zip.write_all(&fs::read(p).map_err(|e| format!("读取图片失败: {}", e))?)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        for icon_path in &referenced_icons {
            let p = std::path::Path::new(icon_path);
            if p.is_file() {
                if let Some(filename) = p.file_name() {
                    let zip_name = format!("icons/{}", filename.to_string_lossy());
                    zip.start_file(&zip_name, options).map_err(|e| e.to_string())?;
                    zip.write_all(&fs::read(p).map_err(|e| format!("读取图标失败: {}", e))?)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }

    // 导出剪贴板引用的文件和视频（去重，跳过已失效，按类型分文件夹）
    // 同一内容（blake3 hash 相同）只保存一份，多个路径共享同一 ZIP 条目
    let rows = repo.get_file_items_for_export().map_err(|e| e.to_string())?;
    let mut seen_paths = std::collections::HashSet::<String>::new();
    let mut seen_zip_paths = std::collections::HashSet::<String>::new();
    // files_manifest: 原始绝对路径 -> ZIP 内相对路径（多个路径可指向同一 zip_rel）
    let mut files_manifest = std::collections::HashMap::<String, String>::new();
    // hash -> 已写入的 zip_rel，用于内容级去重
    let mut hash_to_zip_rel = std::collections::HashMap::<String, String>::new();
    let mut files_exported = 0u32;
    let mut files_deduped = 0u32;

    for (content_type, paths_json, files_valid) in &rows {
        if *files_valid == Some(false) {
            continue;
        }
        let folder = if content_type == "video" { "videos" } else { "files" };
        let paths: Vec<String> = serde_json::from_str(paths_json).unwrap_or_default();
        for abs_path in &paths {
            if !seen_paths.insert(abs_path.clone()) {
                continue;
            }
            let p = std::path::Path::new(abs_path);
            if !p.is_file() {
                continue;
            }
            let data = match fs::read(p) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let hash = blake3::hash(&data).to_hex().to_string();

            // 相同内容已写入过 ZIP，直接复用其路径
            if let Some(existing_zip_rel) = hash_to_zip_rel.get(&hash) {
                files_manifest.insert(abs_path.clone(), existing_zip_rel.clone());
                files_deduped += 1;
                continue;
            }

            let filename = p.file_name().unwrap_or_default().to_string_lossy();
            // 使用文件内容 hash 作为前缀避免同名冲突
            let mut zip_rel = format!("{}/{}/{}", folder, &hash[..8], filename);
            // 防止 hash 前缀+文件名碰撞导致 ZIP 内重复路径
            let mut counter = 1u32;
            while !seen_zip_paths.insert(zip_rel.clone()) {
                zip_rel = format!("{}/{}_{}/{}", folder, &hash[..8], counter, filename);
                counter += 1;
            }
            zip.start_file(&zip_rel, options).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
            files_manifest.insert(abs_path.clone(), zip_rel.clone());
            hash_to_zip_rel.insert(hash, zip_rel);
            files_exported += 1;
        }
    }

    // 写入文件映射表
    if !files_manifest.is_empty() {
        let manifest_json = serde_json::to_string_pretty(&files_manifest)
            .map_err(|e| format!("序列化文件映射失败: {}", e))?;
        zip.start_file("files_manifest.json", options).map_err(|e| e.to_string())?;
        zip.write_all(manifest_json.as_bytes()).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;

    let size = fs::metadata(&dest_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let mut msg = format!("导出成功 ({})", format_size(size));
    if files_exported > 0 {
        msg.push_str(&format!("，含 {} 个文件", files_exported));
        if files_deduped > 0 {
            msg.push_str(&format!("（去重 {} 个）", files_deduped));
        }
    }
    Ok(msg)
}

#[tauri::command]
pub async fn import_data(app: tauri::AppHandle) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Read;
    use tauri_plugin_dialog::DialogExt;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    demote_windows_for_dialog(&app);
    let src = app
        .dialog()
        .file()
        .set_title("导入数据")
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_pick_file();
    restore_windows_after_dialog(&app);

    let src_path = match src {
        Some(p) => p.to_string(),
        None => return Err("用户取消了导入".to_string()),
    };

    let file = File::open(&src_path).map_err(|e| format!("打开文件失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("无效的 ZIP 文件: {}", e))?;

    let has_db = (0..archive.len()).any(|i| {
        archive
            .by_index(i)
            .map(|f| f.name() == "clipboard.db")
            .unwrap_or(false)
    });
    if !has_db {
        return Err("ZIP 文件中未找到 clipboard.db，不是有效的备份文件".to_string());
    }

    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    // 先提取 files_manifest.json（如果存在）
    let files_manifest: std::collections::HashMap<String, String> = {
        match archive.by_name("files_manifest.json") {
            Ok(mut entry) => {
                let mut json = String::new();
                entry.read_to_string(&mut json).unwrap_or_default();
                serde_json::from_str(&json).unwrap_or_default()
            }
            Err(_) => std::collections::HashMap::new(),
        }
    };

    // 反向映射：ZIP 相对路径 -> 所有原始绝对路径（同一 hash 的文件共享同一条目）
    let mut reverse_manifest: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (abs_path, zip_rel) in &files_manifest {
        reverse_manifest.entry(zip_rel.clone()).or_default().push(abs_path.clone());
    }

    let mut files_extracted = 0u32;
    let mut files_restored = 0u32;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        let rel_path = match sanitize_zip_relative_path(&name) {
            Some(path) => path,
            None => {
                tracing::warn!("Skipping unsafe zip entry path: {}", name);
                continue;
            }
        };

        // 跳过 manifest 自身和临时数据库文件
        if name == "files_manifest.json" {
            continue;
        }
        if rel_path.ends_with("clipboard.db-wal") || rel_path.ends_with("clipboard.db-shm") {
            continue;
        }

        // 文件/视频：读取一次，释放到所有原始绝对路径
        if name.starts_with("files/") || name.starts_with("videos/") {
            if let Some(original_paths) = reverse_manifest.get(&name) {
                if !entry.is_dir() {
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                    for original_path in original_paths {
                        let out = std::path::PathBuf::from(original_path);
                        if let Some(parent) = out.parent() {
                            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        fs::write(&out, &buf)
                            .map_err(|e| format!("写入 {} 失败: {}", original_path, e))?;
                        files_restored += 1;
                    }
                    files_extracted += 1;
                }
            }
            continue;
        }

        // 确定输出路径
        let out_path = if rel_path == std::path::Path::new("clipboard.db") {
            data_dir.join("clipboard.db.import")
        } else {
            // images/, icons/ 等资产目录恢复到 data_dir
            data_dir.join(&rel_path)
        };

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            fs::write(&out_path, &buf)
                .map_err(|e| format!("写入 {} 失败: {}", name, e))?;
            files_extracted += 1;
        }
    }

    let mut msg = format!("导入成功，共恢复 {} 个文件", files_extracted);
    if files_restored > 0 {
        msg.push_str(&format!("（含 {} 个文件/视频已释放到原始路径）", files_restored));
    }
    msg.push_str("，应用即将重启");
    Ok(msg)
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    crate::commands::window::save_main_window_placement(&app);
    if crate::admin_launch::restart_app() {
        app.exit(0);
    } else {
        tauri::process::restart(&app.env());
    }
}
