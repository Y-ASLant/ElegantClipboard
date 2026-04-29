use crate::database::ClipboardRepository;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tracing::{debug, info};

use super::{
    clipboard::simulate_paste, hide_main_window_if_not_pinned, with_paused_monitor, AppState,
};

// ============ 文件校验命令 ============

/// 文件检查结果（存在性与是否为目录）
#[derive(serde::Serialize)]
pub struct FileCheckResult {
    pub exists: bool,
    pub is_dir: bool,
}

/// 并行检查文件是否存在，返回路径→结果映射。
#[tauri::command]
pub async fn check_files_exist(
    paths: Vec<String>,
) -> Result<HashMap<String, FileCheckResult>, String> {
    use rayon::prelude::*;
    use std::path::Path;

    let result: HashMap<String, FileCheckResult> = paths
        .par_iter()
        .map(|path| {
            let p = Path::new(path);
            let exists = p.exists();
            let is_dir = exists && p.is_dir();
            (path.clone(), FileCheckResult { exists, is_dir })
        })
        .collect();

    Ok(result)
}

/// 批量刷新所有 files 类型条目的 files_valid 字段，返回实际变化的行数
#[tauri::command]
pub async fn refresh_files_validity(
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    use rayon::prelude::*;
    use std::path::Path;

    let repo = ClipboardRepository::new(&state.db);
    let file_items = repo.get_file_items_for_validity_check().map_err(|e| e.to_string())?;

    if file_items.is_empty() {
        return Ok(0);
    }

    let updates: Vec<(i64, bool)> = file_items
        .par_iter()
        .map(|(id, file_paths_json, _current)| {
            let paths: Vec<String> = serde_json::from_str(file_paths_json).unwrap_or_default();
            let all_exist = !paths.is_empty() && paths.iter().all(|p| Path::new(p).exists());
            (*id, all_exist)
        })
        .collect();

    let changed = repo.batch_update_files_valid(&updates).map_err(|e| e.to_string())?;
    if changed > 0 {
        debug!("refresh_files_validity: {} items changed", changed);
    }
    Ok(changed)
}

// ============ 文件操作命令 ============

/// 在系统文件管理器中定位并高亮显示文件
#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<(), String> {
    use std::path::Path;

    let path = Path::new(&path);

    // 使用 /select 参数在资源管理器中高亮文件
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        debug!("show_in_explorer: {}", path_str);
        std::process::Command::new("explorer.exe")
            .args(["/select,", &path_str])
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = path.parent().unwrap_or(path);
        if std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .is_err()
        {
            std::process::Command::new("nautilus")
                .arg(&path.to_string_lossy().to_string())
                .spawn()
                .map_err(|e| format!("Failed to open file manager: {}", e))?;
        }
    }

    Ok(())
}

/// 将文件路径作为文本写入剪贴板并粘贴
#[tauri::command]
pub async fn paste_as_path(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    let paths_text = if item.content_type == "files" || item.content_type == "video" {
        if let Some(ref paths_json) = item.file_paths {
            let paths: Vec<String> = serde_json::from_str(paths_json)
                .map_err(|e| format!("Failed to parse file paths: {}", e))?;
            paths.join("\n")
        } else {
            return Err("No file paths found".to_string());
        }
    } else {
        return Err("Item is not a file type".to_string());
    };

    with_paused_monitor(&state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        clipboard
            .set_text(&paths_text)
            .map_err(|e| format!("Failed to set clipboard text: {}", e))?;

        hide_main_window_if_not_pinned(&app);

        std::thread::sleep(std::time::Duration::from_millis(50));
        simulate_paste()?;

        debug!("Pasted file path as text for item {}", id);
        Ok(())
    })
}

/// 通过系统另存为对话框保存文件
#[tauri::command]
pub async fn save_file_as(app: tauri::AppHandle, source_path: String) -> Result<bool, String> {
    use std::path::Path;
    use tauri_plugin_dialog::DialogExt;

    let src = Path::new(&source_path);
    if !src.exists() {
        return Err("源文件不存在".to_string());
    }

    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    let dest = app
        .dialog()
        .file()
        .set_title("另存为")
        .set_file_name(&file_name)
        .blocking_save_file();

    match dest {
        Some(dest_path) => {
            let dest_str = dest_path.to_string();
            std::fs::copy(&source_path, &dest_str).map_err(|e| format!("保存失败: {}", e))?;
            info!("File saved: {} -> {}", source_path, dest_str);
            Ok(true)
        }
        None => {
            debug!("save_file_as: user cancelled");
            Ok(false)
        }
    }
}

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpeg", "mpg",
];

fn is_video_path(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&ext.as_str())
}

/// 获取数据目录大小明细（数据库+图片+文件+视频）
/// 文件/视频按实际磁盘路径去重，失效条目不计入大小但标注数量
#[tauri::command]
pub async fn get_data_size(
    state: State<'_, Arc<AppState>>,
) -> Result<DataSizeInfo, String> {
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();

    let db_size = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"]
        .iter()
        .map(|name| {
            std::fs::metadata(data_dir.join(name))
                .map(|m| m.len())
                .unwrap_or(0)
        })
        .sum::<u64>();

    // 仅统计数据库引用的图片文件（跳过孤立的磁盘文件，按内容 hash 去重）
    let (images_size, images_count) = {
        let repo_for_images = ClipboardRepository::new(&state.db);
        let referenced = repo_for_images.get_all_image_paths().unwrap_or_default();
        let mut size = 0u64;
        let mut count = 0u64;
        let mut seen_hashes = std::collections::HashSet::<String>::new();
        for path in &referenced {
            if let Ok(data) = std::fs::read(path) {
                let hash = blake3::hash(&data).to_hex().to_string();
                if seen_hashes.insert(hash) {
                    size += data.len() as u64;
                    count += 1;
                }
            }
        }
        (size, count)
    };

    // 从数据库查询 files/video 类型条目
    // 按文件内容 hash 去重计算实际大小，失效条目只计数不计大小
    let (files_size, files_count, files_invalid_count, videos_size, videos_count, videos_invalid_count) = {
        let repo = ClipboardRepository::new(&state.db);
        let rows = repo.get_files_stats_with_validity().map_err(|e| e.to_string())?;

        let mut seen_hashes = std::collections::HashSet::<String>::new();
        let mut f_size = 0u64;
        let mut f_count = 0u64;
        let mut f_invalid = 0u64;
        let mut v_size = 0u64;
        let mut v_count = 0u64;
        let mut v_invalid = 0u64;

        for (paths_json, files_valid) in rows {
            let paths: Vec<String> = serde_json::from_str(&paths_json).unwrap_or_default();
            let is_video = !paths.is_empty() && paths.iter().all(|p| is_video_path(p));
            let is_invalid = files_valid == Some(false);

            if is_invalid {
                if is_video { v_invalid += 1; } else { f_invalid += 1; }
                continue;
            }

            // 有效条目：按内容 hash 去重后累计实际磁盘大小和数量
            for p in &paths {
                if let Ok(data) = std::fs::read(p) {
                    let hash = blake3::hash(&data).to_hex().to_string();
                    if seen_hashes.insert(hash) {
                        let file_size = data.len() as u64;
                        if is_video {
                            v_size += file_size;
                            v_count += 1;
                        } else {
                            f_size += file_size;
                            f_count += 1;
                        }
                    }
                }
            }
        }
        (f_size, f_count, f_invalid, v_size, v_count, v_invalid)
    };

    Ok(DataSizeInfo {
        db_size,
        images_size,
        images_count,
        files_size,
        files_count,
        files_invalid_count,
        videos_size,
        videos_count,
        videos_invalid_count,
        total_size: db_size + images_size + files_size + videos_size,
    })
}

#[derive(serde::Serialize)]
pub struct DataSizeInfo {
    pub db_size: u64,
    pub images_size: u64,
    pub images_count: u64,
    pub files_size: u64,
    pub files_count: u64,
    pub files_invalid_count: u64,
    pub videos_size: u64,
    pub videos_count: u64,
    pub videos_invalid_count: u64,
    pub total_size: u64,
}

/// 获取文件详情
#[tauri::command]
pub async fn get_file_details(path: String) -> Result<FileDetails, String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&path);
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to get file metadata: {}", e))?;

    let file_type = if metadata.is_dir() {
        "folder".to_string()
    } else if metadata.is_file() {
        path.extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_else(|| "FILE".to_string())
    } else {
        "unknown".to_string()
    };

    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let created = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    Ok(FileDetails {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: path.to_string_lossy().to_string(),
        size: metadata.len() as i64,
        file_type,
        is_dir: metadata.is_dir(),
        modified_at: modified,
        created_at: created,
    })
}

#[derive(serde::Serialize)]
pub struct FileDetails {
    name: String,
    path: String,
    size: i64,
    file_type: String,
    is_dir: bool,
    modified_at: Option<i64>,
    created_at: Option<i64>,
}
