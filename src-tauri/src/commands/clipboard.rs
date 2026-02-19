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
    use clipboard_rs::common::RustImage;
    use clipboard_rs::{Clipboard, ClipboardContext, RustImageData};

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
/// Fast path (O(n)): lowercase entire text once, find byte offset, convert to char index.
/// This works for CJK/ASCII where to_lowercase() preserves byte offsets.
/// Fallback (O(n*k)): sliding-window per-char comparison for rare Unicode edge cases
/// (e.g. Turkish İ where lowercasing changes byte length).
fn extract_keyword_context(text: &str, keyword: &str, max_len: usize) -> String {
    let keyword_lower = keyword.to_lowercase();

    // Fast path: lowercase entire text, find with byte-level search
    let text_lower = text.to_lowercase();
    let keyword_char_pos = if let Some(byte_pos) = text_lower.find(&keyword_lower) {
        // Convert byte position in text_lower to char index
        let char_idx_in_lower = text_lower[..byte_pos].chars().count();
        // Verify: check that the same char index in `text` actually matches.
        // For CJK/ASCII this is always true; only fails for rare Unicode case mappings.
        let mut ci = text.char_indices().skip(char_idx_in_lower);
        let valid = if let Some((bs, _)) = ci.next() {
            // Find byte_end by advancing keyword_lower.chars().count() more chars
            let kw_char_len = keyword_lower.chars().count();
            let be = ci
                .nth(kw_char_len.saturating_sub(2))
                .map(|(b, _)| b)
                .unwrap_or(text.len());
            // Cheap verification: the slice at this char position should lowercase-match
            text.get(bs..be)
                .map(|s| s.to_lowercase() == keyword_lower)
                .unwrap_or(false)
        } else {
            false
        };
        if valid {
            Some(char_idx_in_lower)
        } else {
            // Fallback: sliding window (rare path)
            find_keyword_char_pos_slow(text, &keyword_lower)
        }
    } else {
        None
    };

    let keyword_char_len = keyword_lower.chars().count();
    let keyword_char_pos = match keyword_char_pos {
        Some(pos) => pos,
        None => {
            // Keyword not in text_content (matched via file_paths); return original preview truncation
            return text.chars().take(max_len).collect();
        }
    };

    build_context_snippet(text, keyword_char_pos, keyword_char_len, max_len)
}

/// Slow fallback: O(n*k) sliding window to find keyword char position.
/// Only called for rare Unicode where to_lowercase() shifts byte positions.
fn find_keyword_char_pos_slow(text: &str, keyword_lower: &str) -> Option<usize> {
    let keyword_char_len = keyword_lower.chars().count();
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let n = char_indices.len();
    for i in 0..n {
        if i + keyword_char_len > n {
            break;
        }
        let bs = char_indices[i].0;
        let be = if i + keyword_char_len < n {
            char_indices[i + keyword_char_len].0
        } else {
            text.len()
        };
        if text[bs..be].to_lowercase() == *keyword_lower {
            return Some(i);
        }
    }
    None
}

/// Build the `...context...` snippet given char-level position info.
fn build_context_snippet(
    text: &str,
    keyword_char_pos: usize,
    keyword_char_len: usize,
    max_len: usize,
) -> String {
    let char_indices: Vec<(usize, char)> = text.char_indices().collect();
    let text_char_count = char_indices.len();

    let context_before = max_len / 3;
    let start_char = keyword_char_pos.saturating_sub(context_before);
    let end_char =
        (keyword_char_pos + keyword_char_len + max_len - context_before).min(text_char_count);

    if end_char <= start_char {
        return text.chars().take(max_len).collect();
    }

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

/// Simulate Ctrl+V paste keystroke using the Windows `SendInput` API.
///
/// If the user is still holding Alt (e.g. from an Alt+1 quick-paste shortcut)
/// we release Alt before sending Ctrl+V and **re-press Alt afterwards** so the
/// OS still considers it held.  This allows the user to keep Alt down and tap
/// a number key repeatedly to paste multiple items.
#[cfg(target_os = "windows")]
pub fn simulate_paste() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU, VK_SHIFT, VK_CONTROL, VK_V,
    };

    fn is_key_pressed(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) < 0 }
    }

    fn send_key(vk: u16, up: bool) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32); }
    }

    /// Release a modifier key if the user is currently holding it.
    /// Retries up to 20 times with 5ms delay to ensure the OS processes the release.
    fn release_if_held(vk: u16) -> bool {
        if !is_key_pressed(vk) {
            return false;
        }
        for _ in 0..20 {
            if !is_key_pressed(vk) {
                break;
            }
            send_key(vk, true); // key-up
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        true
    }

    // --- 1. Release modifiers the user may still be holding ---------------
    //     Alt and Shift must be released before Ctrl+V, otherwise the
    //     target app receives Ctrl+Shift+V or Ctrl+Alt+V.
    let user_alt = release_if_held(VK_MENU.0);
    let user_shift = release_if_held(VK_SHIFT.0);

    // --- 2. Send Ctrl+V --------------------------------------------------
    let user_ctrl = is_key_pressed(VK_CONTROL.0);
    if !user_ctrl {
        send_key(VK_CONTROL.0, false); // Ctrl down
    }
    send_key(VK_V.0, false); // V down
    std::thread::sleep(std::time::Duration::from_millis(8));
    send_key(VK_V.0, true);  // V up
    if !user_ctrl {
        send_key(VK_CONTROL.0, true); // Ctrl up
    }

    // --- 3. Re-press modifiers so continuous shortcut presses still work --
    if user_shift {
        send_key(VK_SHIFT.0, false); // Shift down
    }
    if user_alt {
        send_key(VK_MENU.0, false); // Alt down
        // Brief Ctrl tap to reset internal modifier state
        send_key(VK_CONTROL.0, false);
        send_key(VK_CONTROL.0, true);
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;

    // Release modifier keys the user might still be holding.
    for m in [Key::Alt, Key::Shift, Key::Meta, Key::Control] {
        let _ = enigo.key(m, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {}", e))?;

    let click_result = enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {}", e));

    let release_result = enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {}", e));

    if let Err(click_error) = click_result {
        if let Err(release_error) = release_result {
            return Err(format!(
                "{}; additionally failed to release modifier: {}",
                click_error, release_error
            ));
        }
        return Err(click_error);
    }

    release_result?;

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
        let keyword_lower = keyword.to_lowercase();
        for item in &mut items {
            if let Some(ref text) = item.text_content {
                // Only replace preview if keyword is NOT in the original preview
                let preview_has_match = item
                    .preview
                    .as_ref()
                    .map(|p| p.to_lowercase().contains(&keyword_lower))
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
pub async fn delete_clipboard_item(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);

    if let Ok(Some(item)) = repo.get_by_id(id) {
        repo.delete(id).map_err(|e| e.to_string())?;
        if let Some(ref image_path) = item.image_path {
            crate::clipboard::cleanup_image_files(&[image_path.clone()]);
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
    let deleted_files = crate::clipboard::cleanup_image_files(&image_paths);

    info!(
        "Cleared {} clipboard items and {} image files",
        deleted, deleted_files
    );
    Ok(deleted)
}

// ============ Edit Commands ============

/// Update text content of a clipboard item (edit feature)
/// Returns true if the item was deleted (empty content), false if updated.
#[tauri::command]
pub async fn update_text_content(
    state: State<'_, Arc<AppState>>,
    id: i64,
    new_text: String,
) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    if new_text.trim().is_empty() {
        repo.delete(id).map_err(|e| e.to_string())?;
        debug!("Deleted empty item {}", id);
        Ok(true)
    } else {
        repo.update_text_content(id, &new_text)
            .map_err(|e| e.to_string())?;
        debug!("Updated text content for item {}", id);
        Ok(false)
    }
}

// ============ Copy & Paste Commands ============

/// Copy item to system clipboard
#[tauri::command]
pub async fn copy_to_clipboard(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
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

    paste_item_to_active_window(&state, &app, &item)?;
    debug!("Pasted item {} to active window", id);
    Ok(())
}

/// Paste the item at quick slot position (1-9) to active window.
/// Slot ordering follows the default list order:
/// pinned first, then by sort_order desc, then created_at desc.
///
/// Uses `get_by_position` (SELECT *) instead of `list()` which returns
/// NULL text_content for IPC efficiency — we need the actual content here.
pub fn quick_paste_by_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
) -> Result<(), String> {
    if !(1..=9).contains(&slot) {
        return Err("Quick paste slot must be between 1 and 9".to_string());
    }

    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_position((slot - 1) as usize)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No clipboard item available for slot {}", slot))?;

    paste_item_to_active_window(state, app, &item)?;
    debug!("Quick pasted slot {} with item {}", slot, item.id);
    Ok(())
}

/// Shared paste execution:
/// 1) write content to system clipboard
/// 2) hide app window (if not pinned)
/// 3) simulate Ctrl+V to active app
fn paste_item_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    item: &ClipboardItem,
) -> Result<(), String> {
    with_paused_monitor(state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        set_clipboard_content(item, &mut clipboard)?;

        hide_main_window_if_not_pinned(app);

        std::thread::sleep(std::time::Duration::from_millis(50));
        simulate_paste()?;
        Ok(())
    })
}
