use crate::database::{Tag, TagRepository};
use std::sync::Arc;
use tauri::State;

use super::AppState;

#[tauri::command]
pub async fn get_tags(state: State<'_, Arc<AppState>>) -> Result<Vec<Tag>, String> {
    let repo = TagRepository::new(&state.db);
    repo.list_with_count().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_tag(state: State<'_, Arc<AppState>>, name: String) -> Result<Tag, String> {
    let repo = TagRepository::new(&state.db);
    repo.create(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_tag(state: State<'_, Arc<AppState>>, id: i64, name: String) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.rename(id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, Arc<AppState>>, id: i64) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_tag_to_item(state: State<'_, Arc<AppState>>, item_id: i64, tag_id: i64) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.add_tag_to_item(item_id, tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_tag_from_item(state: State<'_, Arc<AppState>>, item_id: i64, tag_id: i64) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.remove_tag_from_item(item_id, tag_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_item_tags(state: State<'_, Arc<AppState>>, item_id: i64) -> Result<Vec<Tag>, String> {
    let repo = TagRepository::new(&state.db);
    repo.get_tags_for_item(item_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_tags(state: State<'_, Arc<AppState>>, tag_ids: Vec<i64>) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.reorder_tags(&tag_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reorder_tag_items(state: State<'_, Arc<AppState>>, tag_id: i64, item_ids: Vec<i64>) -> Result<(), String> {
    let repo = TagRepository::new(&state.db);
    repo.reorder_tag_items(tag_id, &item_ids).map_err(|e| e.to_string())
}
