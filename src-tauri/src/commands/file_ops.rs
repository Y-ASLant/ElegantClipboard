use crate::database::ClipboardRepository;
use crate::input_monitor;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Manager, State};
use tracing::debug;

use super::{clipboard::simulate_paste, AppState};

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

/// Show file/folder in system file explorer (Windows Explorer, Finder, etc.)
#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<(), String> {
    use std::path::Path;
    use std::process::Command;

    let path = Path::new(&path);

    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        Command::new("explorer.exe")
            .args(["/select,", &path_str])
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        let parent = path.parent().unwrap_or(path);
        if Command::new("xdg-open").arg(parent).spawn().is_err() {
            Command::new("nautilus")
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

    state.monitor.pause();

    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
    clipboard
        .set_text(&paths_text)
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;

    // Hide window (skip if pinned)
    if !input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    simulate_paste()?;

    let monitor = state.monitor.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        monitor.resume();
    });

    debug!("Pasted file path as text for item {}", id);
    Ok(())
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
