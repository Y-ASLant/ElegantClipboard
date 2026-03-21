use crate::commands::AppState;
use crate::config::{self, AppConfig};
use crate::database;
use tauri::Manager;

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

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &std::path::Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    use std::io::Write;

    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let buf = std::fs::read(&path)
                .map_err(|e| format!("读取 {:?} 失败: {}", path, e))?;
            zip.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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

    for attempt in 1..=10 {
        let deleted = ["", "-wal", "-shm"].iter().all(|ext| {
            let f = db_dir.join(format!("clipboard.db{ext}"));
            !f.exists() || fs::remove_file(&f).is_ok()
        });

        if deleted && fs::rename(&staging, db_path).is_ok() {
            tracing::info!("Import staging applied (attempt {attempt})");
            return;
        }

        if attempt < 10 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    tracing::error!("Rename failed after 10 attempts, trying copy fallback");
    if fs::copy(&staging, db_path).is_ok() {
        let _ = fs::remove_file(&staging);
        tracing::info!("Import applied via copy fallback");
    } else {
        tracing::error!("Import staging completely failed");
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
    let dest = app
        .dialog()
        .file()
        .set_title("导出数据")
        .set_file_name(&default_name)
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_save_file();

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

    add_dir_to_zip(&mut zip, &data_dir.join("images"), "images", options)?;

    add_dir_to_zip(&mut zip, &data_dir.join("icons"), "icons", options)?;

    zip.finish().map_err(|e| e.to_string())?;

    let size = fs::metadata(&dest_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(format!("导出成功 ({})", format_size(size)))
}

#[tauri::command]
pub async fn import_data(app: tauri::AppHandle) -> Result<String, String> {
    use std::fs::{self, File};
    use std::io::Read;
    use tauri_plugin_dialog::DialogExt;

    let config = AppConfig::load();
    let data_dir = config.get_data_dir();

    let src = app
        .dialog()
        .file()
        .set_title("导入数据")
        .add_filter("ZIP 压缩文件", &["zip"])
        .blocking_pick_file();

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

    let mut files_extracted = 0u32;

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

        // 跳过临时数据库文件，仅导入 clipboard.db 和资产目录
        if rel_path.ends_with("clipboard.db-wal") || rel_path.ends_with("clipboard.db-shm") {
            continue;
        }
        let out_path = if rel_path == std::path::Path::new("clipboard.db") {
            data_dir.join("clipboard.db.import")
        } else {
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

    Ok(format!("导入成功，共恢复 {} 个文件，应用即将重启", files_extracted))
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
