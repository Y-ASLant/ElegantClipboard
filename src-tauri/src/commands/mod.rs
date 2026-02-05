use crate::clipboard::ClipboardMonitor;
use crate::database::{
    ClipboardItem, ClipboardRepository, Database, QueryOptions, SettingsRepository,
};
use crate::input_monitor;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Manager, State};
use tracing::{debug, info};

/// App state containing database and clipboard monitor
pub struct AppState {
    pub db: Database,
    pub monitor: ClipboardMonitor,
}

// ============ Helper Functions ============

/// Set clipboard content from a ClipboardItem (shared logic for copy and paste)
fn set_clipboard_content(item: &ClipboardItem, clipboard: &mut arboard::Clipboard) -> Result<(), String> {
    match item.content_type.as_str() {
        "text" | "html" | "rtf" => {
            if let Some(ref text) = item.text_content {
                clipboard.set_text(text.clone())
                    .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
            }
        }
        "image" => {
            if let Some(ref path) = item.image_path {
                let img = image::open(path)
                    .map_err(|e| format!("Failed to open image: {}", e))?;
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                let img_data = arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                };
                clipboard.set_image(img_data)
                    .map_err(|e| format!("Failed to set clipboard image: {}", e))?;
            }
        }
        "files" => {
            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json)
                    .map_err(|e| format!("Failed to parse file paths: {}", e))?;
                // Use clipboard-rs to set files in correct format
                set_clipboard_files(&paths)?;
            }
        }
        _ => {
            return Err("Unsupported content type".to_string());
        }
    }
    Ok(())
}

/// Set files to clipboard using clipboard-rs (for proper file format)
fn set_clipboard_files(paths: &[String]) -> Result<(), String> {
    use clipboard_rs::{Clipboard, ClipboardContext};
    
    let ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {}", e))?;
    
    ctx.set_files(paths.to_vec())
        .map_err(|e| format!("Failed to set clipboard files: {}", e))?;
    
    Ok(())
}

// ============ Clipboard Commands ============

/// Get clipboard items with optional filtering
#[tauri::command]
pub async fn get_clipboard_items(
    state: State<'_, Arc<AppState>>,
    search: Option<String>,
    content_type: Option<String>,
    pinned_only: Option<bool>,
    favorite_only: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<ClipboardItem>, String> {
    let repo = ClipboardRepository::new(&state.db);
    let options = QueryOptions {
        search,
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        limit,
        offset,
    };
    
    let mut items = repo.list(options).map_err(|e| e.to_string())?;
    
    // Fill files_valid for file-type items (parallel check using rayon)
    fill_files_valid(&mut items);
    
    Ok(items)
}

/// Check file existence and fill files_valid field for file-type items
fn fill_files_valid(items: &mut [ClipboardItem]) {
    use rayon::prelude::*;
    use std::path::Path;
    
    // Parallel iteration over items
    items.par_iter_mut().for_each(|item| {
        if item.content_type == "files" {
            if let Some(ref paths_json) = item.file_paths {
                if let Ok(paths) = serde_json::from_str::<Vec<String>>(paths_json) {
                    // Check if ALL files exist
                    let all_exist = paths.iter().all(|p| Path::new(p).exists());
                    item.files_valid = Some(all_exist);
                }
            }
        }
    });
}

/// Get clipboard item by ID
#[tauri::command]
pub async fn get_clipboard_item(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<Option<ClipboardItem>, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.get_by_id(id).map_err(|e| e.to_string())
}

/// Get total item count
#[tauri::command]
pub async fn get_clipboard_count(
    state: State<'_, Arc<AppState>>,
    content_type: Option<String>,
    pinned_only: Option<bool>,
    favorite_only: Option<bool>,
) -> Result<i64, String> {
    let repo = ClipboardRepository::new(&state.db);
    let options = QueryOptions {
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        ..Default::default()
    };
    
    repo.count(options).map_err(|e| e.to_string())
}

/// Toggle pin status
#[tauri::command]
pub async fn toggle_pin(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.toggle_pin(id).map_err(|e| e.to_string())
}

/// Toggle favorite status
#[tauri::command]
pub async fn toggle_favorite(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.toggle_favorite(id).map_err(|e| e.to_string())
}

/// Move clipboard item by swapping sort order with target
#[tauri::command]
pub async fn move_clipboard_item(
    state: State<'_, Arc<AppState>>,
    from_id: i64,
    to_id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.move_item_by_id(from_id, to_id).map_err(|e| e.to_string())?;
    debug!("Moved clipboard item {} to position of {}", from_id, to_id);
    Ok(())
}

/// Delete clipboard item (also deletes associated image file)
#[tauri::command]
pub async fn delete_clipboard_item(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    
    // Get item first to find image path
    if let Ok(Some(item)) = repo.get_by_id(id) {
        // Delete database record
        repo.delete(id).map_err(|e| e.to_string())?;
        
        // Delete associated image file if exists
        if let Some(image_path) = item.image_path {
            if let Err(e) = std::fs::remove_file(&image_path) {
                debug!("Failed to delete image file {}: {}", image_path, e);
            } else {
                debug!("Deleted image file: {}", image_path);
            }
        }
    } else {
        repo.delete(id).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

/// Clear all non-pinned history (also deletes associated image files)
#[tauri::command]
pub async fn clear_history(
    state: State<'_, Arc<AppState>>,
) -> Result<i64, String> {
    let repo = ClipboardRepository::new(&state.db);
    
    // Get image paths before clearing
    let image_paths = repo.get_clearable_image_paths().unwrap_or_default();
    
    // Clear database records
    let deleted = repo.clear_history().map_err(|e| e.to_string())?;
    
    // Delete associated image files
    let mut deleted_files = 0;
    for path in image_paths {
        if let Err(e) = std::fs::remove_file(&path) {
            debug!("Failed to delete image file {}: {}", path, e);
        } else {
            deleted_files += 1;
        }
    }
    
    info!("Cleared {} clipboard items and {} image files", deleted, deleted_files);
    Ok(deleted)
}

/// Copy item to system clipboard
#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo.get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    // Pause monitor temporarily to avoid re-capturing
    state.monitor.pause();
    
    // Set content to clipboard
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Failed to access clipboard: {}", e))?;
    set_clipboard_content(&item, &mut clipboard)?;

    // Resume monitor after a short delay
    let monitor = state.monitor.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        monitor.resume();
    });

    debug!("Copied item {} to clipboard", id);
    Ok(())
}

// ============ Settings Commands ============

/// Get a setting value
#[tauri::command]
pub async fn get_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
) -> Result<Option<String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get(&key).map_err(|e| e.to_string())
}

/// Set a setting value
#[tauri::command]
pub async fn set_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let repo = SettingsRepository::new(&state.db);
    repo.set(&key, &value).map_err(|e| e.to_string())
}

/// Get all settings
#[tauri::command]
pub async fn get_all_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<HashMap<String, String>, String> {
    let repo = SettingsRepository::new(&state.db);
    repo.get_all().map_err(|e| e.to_string())
}

// ============ Monitor Commands ============

/// Pause clipboard monitoring
#[tauri::command]
pub async fn pause_monitor(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.monitor.pause();
    Ok(())
}

/// Resume clipboard monitoring
#[tauri::command]
pub async fn resume_monitor(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.monitor.resume();
    Ok(())
}

/// Get monitor status
#[tauri::command]
pub async fn get_monitor_status(
    state: State<'_, Arc<AppState>>,
) -> Result<MonitorStatus, String> {
    Ok(MonitorStatus {
        running: state.monitor.is_running(),
        paused: state.monitor.is_paused(),
    })
}

#[derive(serde::Serialize)]
pub struct MonitorStatus {
    pub running: bool,
    pub paused: bool,
}

// ============ Database Commands ============

/// Optimize database
#[tauri::command]
pub async fn optimize_database(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.db.optimize().map_err(|e| e.to_string())?;
    info!("Database optimized");
    Ok(())
}

/// Vacuum database
#[tauri::command]
pub async fn vacuum_database(
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.db.vacuum().map_err(|e| e.to_string())?;
    info!("Database vacuumed");
    Ok(())
}

// ============ Folder Commands ============

/// Open folder selection dialog (for settings window)
#[tauri::command]
pub async fn select_folder_for_settings(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    
    let result = app.dialog()
        .file()
        .set_title("选择数据存储文件夹")
        .blocking_pick_folder();
    
    Ok(result.map(|p| p.to_string()))
}

/// Open data folder in file explorer
#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();
    let parent = &data_dir;
    {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("Failed to open folder: {}", e))?;
        }
    }
    Ok(())
}

// ============ Autostart Commands ============

/// Check if autostart is enabled
#[tauri::command]
pub async fn is_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .is_enabled()
        .map_err(|e| format!("Failed to check autostart: {}", e))
}

/// Enable autostart
#[tauri::command]
pub async fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .enable()
        .map_err(|e| format!("Failed to enable autostart: {}", e))
}

/// Disable autostart
#[tauri::command]
pub async fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch()
        .disable()
        .map_err(|e| format!("Failed to disable autostart: {}", e))
}

// ============ Paste Commands ============

/// Paste clipboard item content directly
/// This will: 1. Copy content to clipboard, 2. Hide window, 3. Simulate Ctrl+V
#[tauri::command]
pub async fn paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo.get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    // Pause monitor temporarily to avoid re-capturing
    state.monitor.pause();
    
    // 1. Set content to system clipboard
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Failed to access clipboard: {}", e))?;
    set_clipboard_content(&item, &mut clipboard)?;

    // 2. Hide window and update state (skip if window is pinned)
    if !input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
        }
    }

    // 3. Simulate Ctrl+V after a small delay
    std::thread::sleep(std::time::Duration::from_millis(50));
    simulate_paste()?;
    
    // Resume monitor after a delay
    let monitor = state.monitor.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        monitor.resume();
    });

    debug!("Pasted item {} to active window", id);
    Ok(())
}

/// Simulate Ctrl+V paste keystroke
#[cfg(target_os = "windows")]
fn simulate_paste() -> Result<(), String> {
    use enigo::{Enigo, Key, Keyboard, Settings, Direction};
    
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;
    
    // Press Ctrl+V
    enigo.key(Key::Control, Direction::Press)
        .map_err(|e| format!("Failed to press Ctrl: {}", e))?;
    enigo.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {}", e))?;
    enigo.key(Key::Control, Direction::Release)
        .map_err(|e| format!("Failed to release Ctrl: {}", e))?;
    
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn simulate_paste() -> Result<(), String> {
    use enigo::{Enigo, Key, Keyboard, Settings, Direction};
    
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;
    
    // Use Ctrl+V on Linux, Cmd+V on macOS
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    
    enigo.key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;
    enigo.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {}", e))?;
    enigo.key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e))?;
    
    Ok(())
}

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
pub async fn check_files_exist(paths: Vec<String>) -> Result<HashMap<String, FileCheckResult>, String> {
    use rayon::prelude::*;
    use std::path::Path;
    
    // Parallel check using rayon (much faster for many files or slow storage)
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
        // Use explorer.exe /select to highlight the file
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
        // Try common file managers
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
    let item = repo.get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;
    
    // Get file paths
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
    
    // Pause monitor temporarily
    state.monitor.pause();
    
    // Set path text to clipboard
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Failed to access clipboard: {}", e))?;
    clipboard.set_text(&paths_text)
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
    
    // Hide window (skip if pinned)
    if !input_monitor::is_window_pinned() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
            crate::keyboard_hook::set_window_state(crate::keyboard_hook::WindowState::Hidden);
        }
    }
    
    // Simulate paste
    std::thread::sleep(std::time::Duration::from_millis(50));
    simulate_paste()?;
    
    // Resume monitor
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
    use std::path::Path;
    use std::fs;
    
    let path = Path::new(&path);
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to get file metadata: {}", e))?;
    
    let file_type = if metadata.is_dir() {
        "folder".to_string()
    } else if metadata.is_file() {
        // Get extension
        path.extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_else(|| "FILE".to_string())
    } else {
        "unknown".to_string()
    };
    
    let modified = metadata.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    
    let created = metadata.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    
    Ok(FileDetails {
        name: path.file_name()
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
