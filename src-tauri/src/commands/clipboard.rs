use crate::database::{ClipboardItem, ClipboardRepository};
use std::sync::Arc;
use tauri::State;
use tracing::debug;

use super::{hide_main_window_if_not_pinned, with_paused_monitor, AppState};

// ============ Helper Functions ============

/// Set clipboard content from a ClipboardItem (shared logic for copy and paste)
///
/// Uses clipboard-rs for image/file operations (like EcoPaste's tauri-plugin-clipboard-x)
/// to avoid the expensive image::open() → to_rgba8() → arboard decode chain.
pub(super) fn set_clipboard_content(
    item: &ClipboardItem,
    clipboard: &mut arboard::Clipboard,
) -> Result<(), String> {
    match item.content_type.as_str() {
        "text" | "html" | "rtf" => {
            if let Some(ref text) = item.text_content {
                clipboard
                    .set_text(text.clone())
                    .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
            }
        }
        "image" => {
            if let Some(ref path) = item.image_path {
                set_clipboard_image(path)?;
            }
        }
        "files" => {
            if let Some(ref paths_json) = item.file_paths {
                let paths: Vec<String> = serde_json::from_str(paths_json)
                    .map_err(|e| format!("Failed to parse file paths: {}", e))?;
                set_clipboard_files(&paths)?;
            }
        }
        _ => {
            return Err("Unsupported content type".to_string());
        }
    }
    Ok(())
}

/// Set image to clipboard using clipboard-rs RustImageData::from_path
/// (same approach as EcoPaste's tauri-plugin-clipboard-x write_image)
///
/// This avoids the expensive path of: image::open() → to_rgba8() → arboard::ImageData
/// Instead, clipboard-rs handles the decode and platform conversion internally.
fn set_clipboard_image(path: &str) -> Result<(), String> {
    use clipboard_rs::{Clipboard, ClipboardContext, RustImageData};
    use clipboard_rs::common::RustImage;

    let image = RustImageData::from_path(path)
        .map_err(|e| format!("Failed to load image from path: {}", e))?;

    let ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {}", e))?;

    ctx.set_image(image)
        .map_err(|e| format!("Failed to set clipboard image: {}", e))?;

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

/// Extract a context snippet centered around the first occurrence of `keyword` in `text`.
/// Returns `...prefix KEYWORD suffix...` with the keyword in the middle.
///
/// Uses char-index searching to avoid byte-position mismatches between
/// `text` and `text.to_lowercase()` (e.g. Turkish İ changes byte length on lowercasing).
fn extract_keyword_context(text: &str, keyword: &str, max_len: usize) -> String {
    let keyword_lower = keyword.to_lowercase();
    let keyword_char_len = keyword_lower.chars().count();

    // Find the char-index of the first case-insensitive match
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let text_char_count = char_indices.len();

    let keyword_char_pos = {
        let mut found = None;
        // Sliding window over chars: compare lowercased slices
        'outer: for i in 0..text_char_count {
            if i + keyword_char_len > text_char_count {
                break;
            }
            let byte_start = char_indices[i].0;
            let byte_end = if i + keyword_char_len < text_char_count {
                char_indices[i + keyword_char_len].0
            } else {
                text.len()
            };
            if text[byte_start..byte_end].to_lowercase() == keyword_lower {
                found = Some(i);
                break 'outer;
            }
        }
        found
    };

    let keyword_char_pos = match keyword_char_pos {
        Some(pos) => pos,
        None => {
            // Keyword not in text_content (matched via file_paths); return original preview truncation
            return text.chars().take(max_len).collect();
        }
    };

    let context_before = max_len / 3;
    let start_char = keyword_char_pos.saturating_sub(context_before);
    let end_char = (keyword_char_pos + keyword_char_len + max_len - context_before).min(text_char_count);

    if end_char <= start_char {
        return text.chars().take(max_len).collect();
    }

    // Convert char indices back to byte slicing
    let byte_start = char_indices[start_char].0;
    let byte_end = if end_char < text_char_count {
        char_indices[end_char].0
    } else {
        text.len()
    };

    let slice = &text[byte_start..byte_end];
    let mut result = String::with_capacity(slice.len() + 6);
    if start_char > 0 {
        result.push_str("...");
    }
    result.push_str(slice);
    if end_char < text_char_count {
        result.push_str("...");
    }
    result
}

/// Check file existence and fill files_valid field for file-type items
fn fill_files_valid(items: &mut [ClipboardItem]) {
    use rayon::prelude::*;
    use std::path::Path;

    items.par_iter_mut().for_each(|item| {
        if item.content_type == "files" {
            if let Some(ref paths_json) = item.file_paths {
                if let Ok(paths) = serde_json::from_str::<Vec<String>>(paths_json) {
                    let all_exist = paths.iter().all(|p| Path::new(p).exists());
                    item.files_valid = Some(all_exist);
                }
            }
        }
    });
}

/// Simulate Ctrl+V paste keystroke
#[cfg(target_os = "windows")]
pub fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;

    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| format!("Failed to press Ctrl: {}", e))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {}", e))?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| format!("Failed to release Ctrl: {}", e))?;

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {}", e))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e))?;

    Ok(())
}

// ============ Clipboard CRUD Commands ============

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
    use crate::database::QueryOptions;

    let repo = ClipboardRepository::new(&state.db);
    let is_searching = search.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let search_keyword = search.clone();
    let options = QueryOptions {
        search,
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        limit,
        offset,
    };
    let mut items = repo.list(options).map_err(|e| e.to_string())?;
    // When searching: replace preview with keyword-centered context snippet,
    // then strip text_content to avoid heavy IPC transfer.
    if let Some(ref keyword) = search_keyword {
        for item in &mut items {
            if let Some(ref text) = item.text_content {
                // Only replace preview if keyword is NOT in the original preview
                let preview_has_match = item.preview.as_ref()
                    .map(|p| p.to_lowercase().contains(&keyword.to_lowercase()))
                    .unwrap_or(false);
                if !preview_has_match {
                    item.preview = Some(extract_keyword_context(text, keyword, 200));
                }
            }
            // Strip heavy field after context extraction
            item.text_content = None;
        }
    }
    // Skip expensive file-existence checks during search (disk I/O per file item);
    // only needed for non-search browsing where user might paste files.
    if !is_searching {
        fill_files_valid(&mut items);
    }
    Ok(items)
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
    use crate::database::QueryOptions;

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
pub async fn toggle_pin(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.toggle_pin(id).map_err(|e| e.to_string())
}

/// Toggle favorite status
#[tauri::command]
pub async fn toggle_favorite(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
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
    repo.move_item_by_id(from_id, to_id)
        .map_err(|e| e.to_string())?;
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

    if let Ok(Some(item)) = repo.get_by_id(id) {
        repo.delete(id).map_err(|e| e.to_string())?;
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
pub async fn clear_history(state: State<'_, Arc<AppState>>) -> Result<i64, String> {
    use tracing::info;

    let repo = ClipboardRepository::new(&state.db);
    let image_paths = repo.get_clearable_image_paths().unwrap_or_default();
    let deleted = repo.clear_history().map_err(|e| e.to_string())?;

    let mut deleted_files = 0;
    for path in image_paths {
        if let Err(e) = std::fs::remove_file(&path) {
            debug!("Failed to delete image file {}: {}", path, e);
        } else {
            deleted_files += 1;
        }
    }

    info!(
        "Cleared {} clipboard items and {} image files",
        deleted, deleted_files
    );
    Ok(deleted)
}

// ============ Copy & Paste Commands ============

/// Copy item to system clipboard
#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    with_paused_monitor(&state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        set_clipboard_content(&item, &mut clipboard)?;
        debug!("Copied item {} to clipboard", id);
        Ok(())
    })
}

/// Paste clipboard item content directly
/// This will: 1. Copy content to clipboard, 2. Hide window, 3. Simulate Ctrl+V
#[tauri::command]
pub async fn paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    with_paused_monitor(&state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        set_clipboard_content(&item, &mut clipboard)?;

        hide_main_window_if_not_pinned(&app);

        std::thread::sleep(std::time::Duration::from_millis(50));
        simulate_paste()?;

        debug!("Pasted item {} to active window", id);
        Ok(())
    })
}
