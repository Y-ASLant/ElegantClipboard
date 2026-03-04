use crate::database::{ClipboardRepository, Group, GroupRepository};
use std::sync::Arc;
use tauri::State;
use tracing::debug;

use super::AppState;

/// 获取所有自定义分组（含条目数）
#[tauri::command]
pub async fn get_groups(state: State<'_, Arc<AppState>>) -> Result<Vec<Group>, String> {
    let repo = GroupRepository::new(&state.db);
    repo.list_with_count().map_err(|e| e.to_string())
}

/// 创建自定义分组，返回完整分组对象
#[tauri::command]
pub async fn create_group(
    state: State<'_, Arc<AppState>>,
    name: String,
    color: Option<String>,
) -> Result<Group, String> {
    let repo = GroupRepository::new(&state.db);
    repo.create(&name, color.as_deref())
        .map_err(|e| e.to_string())
}

/// 重命名分组
#[tauri::command]
pub async fn rename_group(
    state: State<'_, Arc<AppState>>,
    id: i64,
    name: String,
) -> Result<(), String> {
    let repo = GroupRepository::new(&state.db);
    repo.rename(id, &name).map_err(|e| e.to_string())
}

/// 更新分组颜色（传 None 清除颜色）
#[tauri::command]
pub async fn update_group_color(
    state: State<'_, Arc<AppState>>,
    id: i64,
    color: Option<String>,
) -> Result<(), String> {
    let repo = GroupRepository::new(&state.db);
    repo.update_color(id, color.as_deref())
        .map_err(|e| e.to_string())
}

/// 删除分组（ON DELETE CASCADE 自动删除该分组的所有 clipboard_items）
#[tauri::command]
pub async fn delete_group(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let clipboard_repo = ClipboardRepository::new(&state.db);
    let image_paths = clipboard_repo
        .get_image_paths_by_group(id)
        .map_err(|e| e.to_string())?;

    let repo = GroupRepository::new(&state.db);
    repo.delete(id).map_err(|e| e.to_string())?;

    let deleted_files = crate::clipboard::cleanup_image_files(&image_paths);
    debug!(
        "Deleted group {} with {} image files cleaned",
        id, deleted_files
    );
    Ok(())
}

/// 将条目移动到指定分组（None = 移回默认分组）
#[tauri::command]
pub async fn move_item_to_group(
    state: State<'_, Arc<AppState>>,
    item_id: i64,
    group_id: Option<i64>,
) -> Result<(), String> {
    let repo = GroupRepository::new(&state.db);
    repo.move_item_to_group(item_id, group_id)
        .map_err(|e| e.to_string())
}
