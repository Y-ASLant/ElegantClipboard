use crate::database::{ClipboardItem, ClipboardRepository};
use std::sync::Arc;
use tauri::State;
use tracing::{debug, info};

use super::{hide_main_window_if_not_pinned, with_paused_monitor, AppState};

/// 将 ClipboardItem 内容写入系统剪贴板
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

/// 使用 clipboard-rs 将图片写入剪贴板（内部处理解码与平台转换）
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

/// 使用 clipboard-rs 将文件列表写入剪贴板
fn set_clipboard_files(paths: &[String]) -> Result<(), String> {
    use clipboard_rs::{Clipboard, ClipboardContext};

    let ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {}", e))?;

    ctx.set_files(paths.to_vec())
        .map_err(|e| format!("Failed to set clipboard files: {}", e))?;

    Ok(())
}

/// 提取以 keyword 首次出现为中心的上下文片段（`...前缀 关键词 后缀...`）。
/// 快速路径 O(n)：整体小写后字节级搜索转字符索引（CJK/ASCII 通用）。
/// 回退路径 O(n*k)：逐字符滑动窗口（处理小写化会改变字节长度的稀有 Unicode）。
fn extract_keyword_context(text: &str, keyword: &str, max_len: usize) -> String {
    let keyword_lower = keyword.to_lowercase();

    let text_lower = text.to_lowercase();
    let keyword_char_pos = if let Some(byte_pos) = text_lower.find(&keyword_lower) {
        let char_idx_in_lower = text_lower[..byte_pos].chars().count();
        let mut ci = text.char_indices().skip(char_idx_in_lower);
        let valid = if let Some((bs, _)) = ci.next() {
            let kw_char_len = keyword_lower.chars().count();
            let be = ci
                .nth(kw_char_len.saturating_sub(1))
                .map(|(b, _)| b)
                .unwrap_or(text.len());
            text.get(bs..be)
                .map(|s| s.to_lowercase() == keyword_lower)
                .unwrap_or(false)
        } else {
            false
        };
        if valid {
            Some(char_idx_in_lower)
        } else {
            find_keyword_char_pos_slow(text, &keyword_lower)
        }
    } else {
        None
    };

    let keyword_char_len = keyword_lower.chars().count();
    let keyword_char_pos = match keyword_char_pos {
        Some(pos) => pos,
        None => {
            return text.chars().take(max_len).collect();
        }
    };

    build_context_snippet(text, keyword_char_pos, keyword_char_len, max_len)
}

/// 慢速回退：O(n*k) 滑动窗口定位关键词字符位置（仅用于稀有 Unicode 场景）。
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

/// 根据字符级位置信息构建上下文片段。
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

/// 并行检查文件类型条目的文件是否存在，填充 files_valid 字段
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

/// 使用 Windows SendInput API 模拟 Ctrl+V 粘贴。
/// 先释放用户可能按住的所有修饰键（Alt/Shift/Win），再发送纯净的 Ctrl+V。
#[cfg(target_os = "windows")]
pub fn simulate_paste() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT,
        KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
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

    /// 若用户正按住修饰键则释放，最多重试 20 次（间隔 5ms）。
    fn release_if_held(vk: u16) {
        for _ in 0..20 {
            if !is_key_pressed(vk) {
                return;
            }
            send_key(vk, true);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
        let fg = unsafe { GetForegroundWindow() };
        let mut buf = [0u16; 256];
        let len = unsafe { GetWindowTextW(fg, &mut buf) } as usize;
        let title = String::from_utf16_lossy(&buf[..len]);
        info!("simulate_paste: foreground hwnd={:?} title=\"{}\"", fg.0, title);
    }

    release_if_held(VK_MENU.0);
    release_if_held(VK_SHIFT.0);
    release_if_held(VK_LWIN.0);
    release_if_held(VK_RWIN.0);

    let user_ctrl = is_key_pressed(VK_CONTROL.0);
    if !user_ctrl {
        send_key(VK_CONTROL.0, false);
    }
    send_key(VK_V.0, false);
    std::thread::sleep(std::time::Duration::from_millis(8));
    send_key(VK_V.0, true);
    if !user_ctrl {
        send_key(VK_CONTROL.0, true);
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Failed to create keyboard simulator: {}", e))?;

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

/// 获取剪贴板条目（支持可选过滤）
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
    if let Some(ref keyword) = search_keyword {
        let keyword_lower = keyword.to_lowercase();
        for item in &mut items {
            if let Some(ref text) = item.text_content {
                let preview_has_match = item
                    .preview
                    .as_ref()
                    .map(|p| p.to_lowercase().contains(&keyword_lower))
                    .unwrap_or(false);
                if !preview_has_match {
                    item.preview = Some(extract_keyword_context(text, keyword, 200));
                }
            }
            item.text_content = None;
        }
    }
    if !is_searching {
        fill_files_valid(&mut items);
    }
    Ok(items)
}

/// 按 ID 获取剪贴板条目
#[tauri::command]
pub async fn get_clipboard_item(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<Option<ClipboardItem>, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.get_by_id(id).map_err(|e| e.to_string())
}

/// 获取条目总数
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

/// 切换固定状态
#[tauri::command]
pub async fn toggle_pin(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.toggle_pin(id).map_err(|e| e.to_string())
}

/// 切换收藏状态
#[tauri::command]
pub async fn toggle_favorite(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, String> {
    let repo = ClipboardRepository::new(&state.db);
    repo.toggle_favorite(id).map_err(|e| e.to_string())
}

/// 与目标条目交换排序位置
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

/// 删除剪贴板条目（同时删除关联图片文件）
#[tauri::command]
pub async fn delete_clipboard_item(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);

    if let Ok(Some(item)) = repo.get_by_id(id) {
        repo.delete(id).map_err(|e| e.to_string())?;
        if let Some(ref image_path) = item.image_path {
            crate::clipboard::cleanup_image_files(std::slice::from_ref(image_path));
        }
    } else {
        repo.delete(id).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// 清空所有历史（包括置顶/收藏，同时删除图片文件）
#[tauri::command]
pub async fn clear_all_history(state: State<'_, Arc<AppState>>) -> Result<i64, String> {
    use tracing::info;

    let repo = ClipboardRepository::new(&state.db);
    let image_paths = repo.get_all_image_paths().unwrap_or_default();
    let deleted = repo.clear_all().map_err(|e| e.to_string())?;
    let deleted_files = crate::clipboard::cleanup_image_files(&image_paths);
    state.db.vacuum().ok();

    info!(
        "Cleared all {} clipboard items and {} image files",
        deleted, deleted_files
    );
    Ok(deleted)
}

/// 清空所有非固定/非收藏历史（同时删除图片文件）
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

/// 更新剪贴板条目的文本内容，内容为空时删除并返回 true
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

/// 将条目复制到系统剪贴板
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

/// 直接粘贴剪贴板条目（写入系统剪贴板 → 隐藏窗口 → 模拟 Ctrl+V）
#[tauri::command]
pub async fn paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    paste_item_to_active_window(&state, &app, &item, close_window.unwrap_or(true))?;
    debug!("Pasted item {} to active window", id);
    Ok(())
}

/// 以纯文本粘贴条目内容（去除 html/rtf 格式）
#[tauri::command]
pub async fn paste_content_as_plain(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Item not found".to_string())?;

    let text = item
        .text_content
        .as_deref()
        .ok_or_else(|| "Item has no text content".to_string())?;

    paste_plain_text_to_active_window(&state, &app, text, close_window.unwrap_or(true))?;
    debug!("Pasted item {} as plain text", id);
    Ok(())
}

/// 将任意文本直接粘贴到当前活动窗口（用于表情、片段等功能）
#[tauri::command]
pub async fn paste_text_direct(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    text: String,
) -> Result<(), String> {
    paste_plain_text_to_active_window(&state, &app, &text, true)?;
    debug!("Pasted direct text ({} chars)", text.len());
    Ok(())
}

/// 粘贴快速槽位（1-9）对应条目到活动窗口。
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

    paste_item_to_active_window(state, app, &item, true)?;
    debug!("Quick pasted slot {} with item {}", slot, item.id);
    Ok(())
}

/// 公共粘贴执行：写剪贴板 → 隐藏窗口 → 模拟 Ctrl+V
fn paste_item_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    item: &ClipboardItem,
    close_window: bool,
) -> Result<(), String> {
    info!("paste_item: id={}, close_window={}", item.id, close_window);
    with_paused_monitor(state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        set_clipboard_content(item, &mut clipboard)?;
        debug!("paste_item: clipboard set ok");

        if close_window {
            hide_main_window_if_not_pinned(app);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        simulate_paste()?;
        debug!("paste_item: simulate_paste ok");
        Ok(())
    })
}

/// 纯文本粘贴：写剪贴板 → 隐藏窗口 → 模拟 Ctrl+V
fn paste_plain_text_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: &str,
    close_window: bool,
) -> Result<(), String> {
    info!("paste_plain_text: len={}, close_window={}", text.len(), close_window);
    with_paused_monitor(state, || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
        clipboard
            .set_text(text)
            .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
        debug!("paste_plain_text: clipboard set ok");

        if close_window {
            hide_main_window_if_not_pinned(app);
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
        simulate_paste()?;
        debug!("paste_plain_text: simulate_paste ok");
        Ok(())
    })
}
