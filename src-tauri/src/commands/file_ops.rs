use crate::database::ClipboardRepository;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tracing::debug;

use super::{
    clipboard::simulate_paste, hide_main_window_if_not_pinned, with_paused_monitor, AppState,
};

// ============ File Validation Commands ============

/// File check result with existence and directory info
#[derive(serde::Serialize)]
pub struct FileCheckResult {
    pub exists: bool,
    pub is_dir: bool,
}

/// Check if files exist on disk (parallel for better performance)
/// Returns a map of file path -> FileCheckResult
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

// ============ File Operation Commands ============

/// Show file/folder in system file explorer, highlighting the file
#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<(), String> {
    use std::path::Path;

    let path = Path::new(&path);

    // show_in_explorer uses /select to highlight the file (different from open_path_in_explorer)
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
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

/// Copy file path as text to clipboard and paste
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

    let paths_text = if item.content_type == "files" {
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

/// Save a file to user-chosen location via system save dialog
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
            std::fs::copy(&source_path, &dest_str)
                .map_err(|e| format!("保存失败: {}", e))?;
            Ok(true)
        }
        None => Ok(false), // User cancelled
    }
}

/// Get data directory size breakdown (database + images)
#[tauri::command]
pub async fn get_data_size() -> Result<DataSizeInfo, String> {
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

    let images_dir = data_dir.join("images");
    let (images_size, images_count) = if images_dir.is_dir() {
        let mut size = 0u64;
        let mut count = 0u64;
        if let Ok(entries) = std::fs::read_dir(&images_dir) {
            for entry in entries.flatten() {
                if entry.path().is_file() {
                    size += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    count += 1;
                }
            }
        }
        (size, count)
    } else {
        (0, 0)
    };

    Ok(DataSizeInfo {
        db_size,
        images_size,
        images_count,
        total_size: db_size + images_size,
    })
}

#[derive(serde::Serialize)]
pub struct DataSizeInfo {
    pub db_size: u64,
    pub images_size: u64,
    pub images_count: u64,
    pub total_size: u64,
}

/// Get file details for display
#[tauri::command]
pub async fn get_file_details(path: String) -> Result<FileDetails, String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&path);
    let metadata =
        fs::metadata(path).map_err(|e| format!("Failed to get file metadata: {}", e))?;

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
