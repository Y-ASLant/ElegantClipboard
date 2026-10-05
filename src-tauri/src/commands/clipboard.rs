use crate::clipboard::format_write::{PreparedClipboard, prepare_item, prepare_text};
use crate::database::{ClipboardItem, ClipboardRepository};
use crate::operation_error::{OperationError, OperationErrorCode};
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tracing::debug;

use super::{AppState, hide_main_window_if_not_pinned, with_paused_monitor};

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
                .map_or(text.len(), |(b, _)| b);
            text.get(bs..be)
                .is_some_and(|s| s.to_lowercase() == keyword_lower)
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
    let Some(keyword_char_pos) = keyword_char_pos else {
        return text.chars().take(max_len).collect();
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

#[cfg(target_os = "windows")]
mod win_keyboard {
    use tracing::info;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, INPUT, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SendInput, VK_INSERT, VK_LWIN, VK_RWIN,
    };

    pub fn is_key_pressed(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(i32::from(vk)) < 0 }
    }

    fn keyboard_input(vk: u16, up: bool) -> INPUT {
        let mut flags = if up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        // 明确发送独立 Insert，而不是数字小键盘 Insert/0；Win 键也使用 E0 标记。
        if vk == VK_INSERT.0 || vk == VK_LWIN.0 || vk == VK_RWIN.0 {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn send_key(vk: u16, up: bool) -> Result<(), String> {
        let input = keyboard_input(vk, up);
        let inserted = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        if inserted != 1 {
            return Err(format!(
                "SendInput failed for key {vk:#x}: inserted {inserted}/1 events"
            ));
        }
        Ok(())
    }

    /// 若用户正按住修饰键则释放，最多重试 20 次（间隔 5ms）。
    pub fn release_if_held(vk: u16) -> Result<(), String> {
        for _ in 0..20 {
            if !is_key_pressed(vk) {
                return Ok(());
            }
            send_key(vk, true)?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if is_key_pressed(vk) {
            return Err(format!("Modifier key {vk:#x} is still pressed"));
        }
        Ok(())
    }

    pub fn send_combo(modifier: u16, key: u16) -> Result<(), String> {
        send_combo_with(modifier, key, is_key_pressed(modifier), send_key)
    }

    fn send_combo_with(
        modifier: u16,
        key: u16,
        user_held_modifier: bool,
        mut send: impl FnMut(u16, bool) -> Result<(), String>,
    ) -> Result<(), String> {
        if !user_held_modifier {
            send(modifier, false)?;
        }
        let result = (|| {
            send(key, false)?;
            std::thread::sleep(std::time::Duration::from_millis(8));
            send(key, true)
        })();
        // 主键注入失败时也释放本次补按的修饰键；不释放用户原本按住的键。
        let release = if !user_held_modifier {
            send(modifier, true)
        } else {
            Ok(())
        };
        result.and(release)
    }

    pub fn log_foreground_window(action: &str) {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
        let fg = unsafe { GetForegroundWindow() };
        let mut buf = [0u16; 256];
        let len = unsafe { GetWindowTextW(fg, &mut buf) } as usize;
        let title = String::from_utf16_lossy(&buf[..len]);
        info!("{action}: foreground hwnd={:?} title=\"{title}\"", fg.0);
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_INSERT, VK_SHIFT, VK_V};

        #[test]
        fn insert_is_extended_on_both_edges_without_changing_modifier_encoding() {
            for up in [false, true] {
                let insert = unsafe { keyboard_input(VK_INSERT.0, up).Anonymous.ki };
                assert_eq!(insert.wVk, VK_INSERT);
                assert!(insert.dwFlags.contains(KEYEVENTF_EXTENDEDKEY));
                assert_eq!(insert.dwFlags.contains(KEYEVENTF_KEYUP), up);
                for key in [VK_SHIFT, VK_CONTROL, VK_V] {
                    let event = unsafe { keyboard_input(key.0, up).Anonymous.ki };
                    assert!(!event.dwFlags.contains(KEYEVENTF_EXTENDEDKEY));
                    assert_eq!(event.dwFlags.contains(KEYEVENTF_KEYUP), up);
                }
            }
        }

        #[test]
        fn failed_main_key_injection_releases_only_the_injected_modifier() {
            for user_held in [false, true] {
                for failed_up in [false, true] {
                    let mut modifier_down = user_held;
                    let result = send_combo_with(VK_SHIFT.0, VK_INSERT.0, user_held, |key, up| {
                        if key == VK_INSERT.0 && up == failed_up {
                            return Err("injection rejected".into());
                        }
                        if key == VK_SHIFT.0 {
                            modifier_down = !up;
                        }
                        Ok(())
                    });
                    assert_eq!(result, Err("injection rejected".into()));
                    assert_eq!(modifier_down, user_held);
                }
            }
        }

        #[test]
        fn failed_modifier_press_does_not_send_the_main_key() {
            let mut main_key_sent = false;
            let result = send_combo_with(VK_SHIFT.0, VK_INSERT.0, false, |key, _| {
                if key == VK_SHIFT.0 {
                    return Err("modifier rejected".into());
                }
                main_key_sent = true;
                Ok(())
            });
            assert!(result.is_err());
            assert!(!main_key_sent);
        }

        #[test]
        fn modifier_release_failure_is_reported() {
            let result = send_combo_with(VK_SHIFT.0, VK_INSERT.0, false, |key, up| {
                if key == VK_SHIFT.0 && up {
                    Err("release rejected".into())
                } else {
                    Ok(())
                }
            });
            assert_eq!(result, Err("release rejected".into()));
        }
    }
}

/// 使用 Windows SendInput API 模拟 Ctrl+组合键。
/// 先释放用户可能按住的所有修饰键（Alt/Shift/Win），再发送纯净的组合键。
#[cfg(target_os = "windows")]
fn simulate_ctrl_combo(key_vk: u16, action: &str) -> Result<(), String> {
    use win_keyboard::{log_foreground_window, release_if_held, send_combo};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    log_foreground_window(action);

    release_if_held(VK_MENU.0)?;
    release_if_held(VK_SHIFT.0)?;
    release_if_held(VK_LWIN.0)?;
    release_if_held(VK_RWIN.0)?;

    send_combo(VK_CONTROL.0, key_vk)
}

pub const PASTE_KEY_SETTING: &str = "paste_key";
pub const PASTE_KEY_CTRL_V: &str = "ctrl_v";
pub const PASTE_KEY_SHIFT_INSERT: &str = "shift_insert";

/// 按用户设置模拟粘贴按键（未知值回退为 Ctrl+V）。
#[cfg(target_os = "windows")]
pub fn simulate_paste_by_key(paste_key: &str) -> Result<(), String> {
    if paste_key == PASTE_KEY_SHIFT_INSERT {
        simulate_shift_insert()
    } else {
        simulate_paste()
    }
}

/// 使用 Windows SendInput API 模拟 Ctrl+V 粘贴。
/// 先释放用户可能按住的所有修饰键（Alt/Shift/Win），再发送纯净的 Ctrl+V。
#[cfg(target_os = "windows")]
pub fn simulate_paste() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_V;
    simulate_ctrl_combo(VK_V.0, "simulate_paste")
}

/// 使用 Windows SendInput API 模拟 Shift+Insert 粘贴。
/// 先释放 Ctrl/Alt/Win，保留或补按 Shift 后发送 Insert。
#[cfg(target_os = "windows")]
fn simulate_shift_insert() -> Result<(), String> {
    use win_keyboard::{log_foreground_window, release_if_held, send_combo};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CONTROL, VK_INSERT, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    log_foreground_window("simulate_shift_insert");

    release_if_held(VK_MENU.0)?;
    release_if_held(VK_CONTROL.0)?;
    release_if_held(VK_LWIN.0)?;
    release_if_held(VK_RWIN.0)?;

    send_combo(VK_SHIFT.0, VK_INSERT.0)
}

/// 使用 Windows SendInput API 模拟 Ctrl+C 复制选中文字。
#[cfg(target_os = "windows")]
pub fn simulate_copy() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_C;
    simulate_ctrl_combo(VK_C.0, "simulate_copy")
}

/// 获取剪贴板条目（支持可选过滤）
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_clipboard_items(
    state: State<'_, Arc<AppState>>,
    search: Option<String>,
    content_type: Option<String>,
    pinned_only: Option<bool>,
    favorite_only: Option<bool>,
    group_id: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<ClipboardItem>, String> {
    use crate::database::QueryOptions;

    let repo = ClipboardRepository::new(&state.db);
    let search_keyword = search.clone();
    let options = QueryOptions {
        search,
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        group_id,
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
                    .is_some_and(|p| p.to_lowercase().contains(&keyword_lower));
                if !preview_has_match {
                    item.preview = Some(extract_keyword_context(text, keyword, 200));
                }
            }
            item.text_content = None;
        }
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
    group_id: Option<i64>,
) -> Result<i64, String> {
    use crate::database::QueryOptions;

    let repo = ClipboardRepository::new(&state.db);
    let options = QueryOptions {
        content_type,
        pinned_only: pinned_only.unwrap_or(false),
        favorite_only: favorite_only.unwrap_or(false),
        group_id,
        ..Default::default()
    };
    repo.count(options).map_err(|e| e.to_string())
}

/// 切换固定状态
#[tauri::command]
pub async fn toggle_pin(state: State<'_, Arc<AppState>>, id: i64) -> Result<bool, OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    let new_state = repo.toggle_pin(id).map_err(OperationError::database)?;
    debug!("Toggle pin: id={}, pinned={}", id, new_state);
    Ok(new_state)
}

/// 切换收藏状态
#[tauri::command]
pub async fn toggle_favorite(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<bool, OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    let new_state = repo.toggle_favorite(id).map_err(OperationError::database)?;
    debug!("Toggle favorite: id={}, favorite={}", id, new_state);
    Ok(new_state)
}

/// 与目标条目交换排序位置
#[tauri::command]
pub async fn move_clipboard_item(
    state: State<'_, Arc<AppState>>,
    from_id: i64,
    to_id: i64,
) -> Result<(), OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    repo.move_item_by_id(from_id, to_id)
        .map_err(OperationError::database)?;
    debug!("Moved clipboard item {} to position of {}", from_id, to_id);
    Ok(())
}

/// 与目标收藏条目交换收藏排序位置
#[tauri::command]
pub async fn move_favorite_clipboard_item(
    state: State<'_, Arc<AppState>>,
    from_id: i64,
    to_id: i64,
) -> Result<(), OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    repo.move_favorite_item_by_id(from_id, to_id)
        .map_err(OperationError::database)?;
    debug!(
        "Moved favorite clipboard item {} to position of {}",
        from_id, to_id
    );
    Ok(())
}

/// 粘贴后置顶：将条目移到非置顶区最前面（sort_order 设为全表最大值 + 1）
#[tauri::command]
pub async fn bump_item_to_top(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    repo.bump_to_top(id).map_err(OperationError::database)?;
    debug!("Bumped clipboard item {} to top", id);
    Ok(())
}

/// 删除剪贴板条目（同时删除关联图片文件）
#[tauri::command]
pub async fn delete_clipboard_item(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    let item = resolve_item(&state, id)?;
    repo.delete(id).map_err(OperationError::database)?;
    let payloads: Vec<String> = item
        .file_payload
        .map(|payload| vec![payload])
        .unwrap_or_default();
    crate::clipboard::cleanup_deleted_assets(
        &item.image_path.map(|path| vec![path]).unwrap_or_default(),
        &payloads,
    );
    debug!(id, content_type = %item.content_type, "deleted clipboard item");
    Ok(())
}

/// 批量删除剪贴板条目（同时删除关联图片文件）
#[tauri::command]
pub async fn batch_delete_clipboard_items(
    state: State<'_, Arc<AppState>>,
    ids: Vec<i64>,
) -> Result<i64, OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    let (deleted, image_paths, file_payloads) =
        repo.batch_delete(&ids).map_err(OperationError::database)?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);
    debug!("Batch deleted {} clipboard items", deleted);
    Ok(deleted)
}

/// 清空所有历史（包括置顶/收藏，同时删除图片文件）
#[tauri::command]
pub async fn clear_all_history(state: State<'_, Arc<AppState>>) -> Result<i64, OperationError> {
    use tracing::info;

    let repo = ClipboardRepository::new(&state.db);
    let image_paths = repo
        .get_all_image_paths()
        .map_err(OperationError::database)?;
    let file_payloads = repo
        .get_all_file_payloads()
        .map_err(OperationError::database)?;
    let deleted = repo.clear_all().map_err(OperationError::database)?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);
    if let Err(error) = state.db.vacuum() {
        tracing::warn!(error = %error, "post-clear database vacuum failed");
    }

    info!(
        "Cleared all {} clipboard items ({} image files)",
        deleted,
        image_paths.len()
    );
    Ok(deleted)
}

/// 清空所有非固定/非收藏历史（同时删除图片文件），按分组
#[tauri::command]
pub async fn clear_history(
    state: State<'_, Arc<AppState>>,
    group_id: Option<i64>,
    content_type: Option<String>,
) -> Result<i64, OperationError> {
    use tracing::info;

    let repo = ClipboardRepository::new(&state.db);
    let image_paths = repo
        .get_clearable_image_paths(group_id, content_type.as_deref())
        .map_err(OperationError::database)?;
    let file_payloads = repo
        .get_clearable_file_payloads(group_id, content_type.as_deref())
        .map_err(OperationError::database)?;
    let deleted = repo
        .clear_history(group_id, content_type.as_deref())
        .map_err(OperationError::database)?;
    crate::clipboard::cleanup_deleted_assets(&image_paths, &file_payloads);

    info!(
        "Cleared {} clipboard items ({} image files) (group: {:?}, content_type: {:?})",
        deleted,
        image_paths.len(),
        group_id,
        content_type
    );
    Ok(deleted)
}

/// 设置当前活动分组（None = 默认分组）
#[tauri::command]
pub async fn set_active_group(
    state: State<'_, Arc<AppState>>,
    group_id: Option<i64>,
) -> Result<(), String> {
    *state.active_group_id.lock() = group_id;
    debug!("Active group set to: {:?}", group_id);
    Ok(())
}

/// 更新剪贴板条目的文本内容，内容为空时删除并返回 true
#[tauri::command]
pub async fn update_text_content(
    state: State<'_, Arc<AppState>>,
    id: i64,
    new_text: String,
) -> Result<bool, OperationError> {
    let repo = ClipboardRepository::new(&state.db);
    let item = resolve_item(&state, id)?;
    if new_text.trim().is_empty() {
        repo.delete(id).map_err(OperationError::database)?;
        debug!(id, "deleted empty item");
        Ok(true)
    } else {
        repo.update_text_content(id, &new_text)
            .map_err(OperationError::database)?;
        if let Some(payload) = item.file_payload {
            crate::clipboard::cleanup_deleted_assets(&[], &[payload]);
        }
        debug!("Updated text content for item {}", id);
        Ok(false)
    }
}

pub(super) fn resolve_item(
    state: &Arc<AppState>,
    id: i64,
) -> Result<ClipboardItem, OperationError> {
    ClipboardRepository::new(&state.db)
        .get_by_id(id)
        .map_err(OperationError::database)?
        .ok_or_else(|| {
            OperationError::new(
                OperationErrorCode::ItemNotFound,
                format!("item {id} absent"),
            )
        })
}

pub(super) fn write_prepared(
    state: &Arc<AppState>,
    prepared: PreparedClipboard,
) -> Result<(), OperationError> {
    with_paused_monitor(state, || {
        let mut clipboard = clipboard_rs::ClipboardContext::new().map_err(|error| {
            OperationError::new(
                OperationErrorCode::ClipboardUnavailable,
                format!("open clipboard: {error}"),
            )
        })?;
        prepared.write(&mut clipboard)
    })
}

#[tauri::command]
pub async fn copy_to_clipboard(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), OperationError> {
    let item = resolve_item(&state, id)?;
    let prepared = prepare_item(&item)?;
    write_prepared(&state, prepared)
}

#[tauri::command]
pub async fn paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), OperationError> {
    let item = resolve_item(&state, id)?;
    paste_item_to_active_window(&state, &app, &item, close_window.unwrap_or(true))
}

#[tauri::command]
pub async fn paste_content_as_plain(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
    close_window: Option<bool>,
) -> Result<(), OperationError> {
    let item = resolve_item(&state, id)?;
    let text = crate::clipboard::format_write::item_plain_text(&item)?;
    paste_plain_text_to_active_window(&state, &app, text, close_window.unwrap_or(true))
}

#[tauri::command]
pub async fn paste_text_direct(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    text: String,
) -> Result<(), OperationError> {
    paste_plain_text_to_active_window(&state, &app, text, true)
}

#[tauri::command]
pub async fn merge_paste_content(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    ids: Vec<i64>,
    separator: Option<String>,
) -> Result<(), OperationError> {
    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        items.push(resolve_item(&state, id)?);
    }
    let prepared =
        crate::clipboard::merge_paste::prepare_merge(&items, separator.as_deref().unwrap_or("\n"))?;
    execute_paste_flow(&state, &app, true, prepared)
}

pub fn quick_paste_by_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
) -> Result<(), OperationError> {
    quick_paste_slot(state, app, slot, false)
}

pub fn quick_paste_favorite_by_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
) -> Result<(), OperationError> {
    quick_paste_slot(state, app, slot, true)
}

fn quick_paste_slot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    slot: u8,
    favorite: bool,
) -> Result<(), OperationError> {
    if !(1..=10).contains(&slot) {
        return Err(OperationError::new(
            OperationErrorCode::InvalidContent,
            "slot must be 1-10",
        ));
    }
    let repo = ClipboardRepository::new(&state.db);
    let group = *state.active_group_id.lock();
    let item = if favorite {
        repo.get_favorite_by_position((slot - 1) as usize, group)
    } else {
        repo.get_by_position((slot - 1) as usize, group)
    }
    .map_err(OperationError::database)?
    .ok_or_else(|| {
        OperationError::new(
            OperationErrorCode::ItemNotFound,
            format!("slot {slot} absent"),
        )
    })?;
    paste_item_to_active_window(state, app, &item, true)
}

fn execute_paste_flow(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    close_window: bool,
    prepared: PreparedClipboard,
) -> Result<(), OperationError> {
    with_paused_monitor(state, || {
        let mut clipboard = clipboard_rs::ClipboardContext::new().map_err(|error| {
            OperationError::new(
                OperationErrorCode::ClipboardUnavailable,
                format!("open clipboard: {error}"),
            )
        })?;
        prepared.write(&mut clipboard)?;
        let main = app.get_webview_window("main");
        let was_visible = main
            .as_ref()
            .is_some_and(|window| window.is_visible().unwrap_or(false));
        super::hide_preview_windows(app);
        if close_window {
            hide_main_window_if_not_pinned(app);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Err(error) = super::run_simulate_paste_with_sound(app) {
            if was_visible && let Some(window) = main {
                let restored = crate::main_thread::run_on_ui_thread(app, move || {
                    if !window.is_visible()? {
                        window.show()?;
                        crate::keyboard_hook::set_window_state(
                            crate::keyboard_hook::WindowState::Visible,
                        );
                        crate::input_monitor::enable_mouse_monitoring();
                        window.emit("window-shown", ())?;
                    }
                    Ok::<(), tauri::Error>(())
                });
                if !matches!(restored, Ok(Ok(()))) {
                    tracing::error!("failed to restore main window after paste input failure");
                }
            }
            return Err(OperationError::new(OperationErrorCode::PasteFailed, error));
        }
        super::hide_preview_windows(app);
        Ok(())
    })
}

fn paste_item_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    item: &ClipboardItem,
    close_window: bool,
) -> Result<(), OperationError> {
    let prepared = prepare_item(item)?;
    execute_paste_flow(state, app, close_window, prepared)
}

pub(super) fn paste_plain_text_to_active_window(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: String,
    close_window: bool,
) -> Result<(), OperationError> {
    let prepared = prepare_text(text)?;
    execute_paste_flow(state, app, close_window, prepared)
}

#[cfg(test)]
mod tests {
    use super::extract_keyword_context;

    #[test]
    fn keyword_at_start() {
        let result = extract_keyword_context("hello world foo bar", "hello", 20);
        assert!(result.contains("hello"), "result: {result}");
    }

    #[test]
    fn keyword_in_middle() {
        let result = extract_keyword_context("aaa bbb ccc ddd eee", "ccc", 15);
        assert!(result.contains("ccc"), "result: {result}");
    }

    #[test]
    fn keyword_at_end() {
        let result = extract_keyword_context("foo bar baz qux", "qux", 15);
        assert!(result.contains("qux"), "result: {result}");
    }

    #[test]
    fn keyword_not_found_returns_prefix() {
        let text = "abcdefghijklmnop";
        let result = extract_keyword_context(text, "xyz", 5);
        assert_eq!(result, text.chars().take(5).collect::<String>());
    }

    #[test]
    fn case_insensitive() {
        let result = extract_keyword_context("Hello World", "hello", 20);
        assert!(result.contains("Hello"), "result: {result}");
    }

    #[test]
    fn cjk_keyword() {
        let result = extract_keyword_context("这是一段中文文本用于测试", "中文", 10);
        assert!(result.contains("中文"), "result: {result}");
    }

    #[test]
    fn cjk_text_with_emoji() {
        let result = extract_keyword_context("测试 🎉 emoji 关键词搜索", "关键词", 15);
        assert!(result.contains("关键词"), "result: {result}");
    }

    #[test]
    fn empty_keyword_returns_prefix() {
        let text = "hello world";
        let result = extract_keyword_context(text, "", 5);
        // 空关键词视为未找到，返回截断前缀（可能带省略号）
        assert!(result.starts_with("hell"), "result: {result}");
    }

    #[test]
    fn empty_text() {
        let result = extract_keyword_context("", "keyword", 10);
        assert_eq!(result, "");
    }

    #[test]
    fn max_len_larger_than_text() {
        let result = extract_keyword_context("short", "short", 100);
        assert_eq!(result, "short");
    }

    #[test]
    fn unicode_boundary_safety() {
        // 多字节字符不应 panic
        let result = extract_keyword_context("émoji 🎉 test", "🎉", 20);
        assert!(result.contains("🎉"), "result: {result}");
    }
}
