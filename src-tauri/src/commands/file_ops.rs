use crate::clipboard::file_clipboard::{
    item_files_all_exist, parse_file_paths, resolve_item_paths,
};
use crate::database::ClipboardRepository;
use crate::file_preview_limits::{
    DEFAULT_MAX_IMAGE_SIZE_KB, is_too_large_for_preview, is_unc_path, preview_limit_bytes,
};
use clipboard_rs::Clipboard as ClipboardTrait;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use tracing::{debug, info};

use super::{AppState, hide_main_window_if_not_pinned, with_paused_monitor};

// ============ 文件校验命令 ============

/// 文件检查结果（存在性与是否为目录）
#[derive(serde::Serialize, Clone)]
pub struct FileCheckResult {
    pub exists: bool,
    pub is_dir: bool,
}

/// 并行检查文件是否存在，返回路径→结果映射。
/// 若提供 `file_payload`，会先解析 staged 回退路径再检查。
#[tauri::command]
pub async fn check_files_exist(
    paths: Vec<String>,
    file_payload: Option<String>,
) -> Result<HashMap<String, FileCheckResult>, String> {
    use rayon::prelude::*;
    use std::path::Path;

    let resolved = if file_payload.is_some() {
        let paths_json = serde_json::to_string(&paths).unwrap_or_default();
        resolve_item_paths(Some(&paths_json), file_payload.as_deref())
    } else {
        paths.clone()
    };

    let resolved_by_original: HashMap<String, String> = paths
        .iter()
        .cloned()
        .zip(resolved.iter().cloned())
        .collect();

    let result: HashMap<String, FileCheckResult> = resolved
        .par_iter()
        .map(|path| {
            let p = Path::new(path);
            let exists = p.exists();
            let is_dir = exists && p.is_dir();
            (path.clone(), FileCheckResult { exists, is_dir })
        })
        .collect();

    // 以原始路径为 key 返回，便于 UI 展示
    Ok(paths
        .into_iter()
        .map(|orig| {
            let check_path = resolved_by_original
                .get(&orig)
                .cloned()
                .unwrap_or(orig.clone());
            let info = result.get(&check_path).cloned().unwrap_or(FileCheckResult {
                exists: false,
                is_dir: false,
            });
            (orig, info)
        })
        .collect())
}

/// 解析条目文件路径（含 staged 回退）并检查有效性
#[derive(serde::Serialize, Clone)]
pub struct ItemFileStatus {
    pub all_exist: bool,
    pub resolved_paths: Vec<String>,
    pub checks: HashMap<String, FileCheckResult>,
    /// 单文件图片超过预览阈值时为 true，前端据此跳过图片预览
    #[serde(default)]
    pub too_large: bool,
}

fn read_max_image_size_kb(db: &crate::database::Database) -> u64 {
    use crate::database::SettingsRepository;
    SettingsRepository::new(db)
        .get("max_image_size_kb")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_IMAGE_SIZE_KB)
}

fn compute_too_large_for_preview(
    item: &crate::database::ClipboardItem,
    resolved: &[String],
    max_image_size_kb: u64,
) -> bool {
    if resolved.len() != 1 {
        return false;
    }
    let path = &resolved[0];
    if is_too_large_for_preview(path, item.byte_size, max_image_size_kb, true) {
        return true;
    }
    if item.byte_size > 0 {
        return false;
    }
    match std::fs::metadata(path) {
        Ok(meta) => meta.len() > preview_limit_bytes(path, max_image_size_kb),
        Err(_) => is_unc_path(path),
    }
}

fn build_item_file_status(
    item: &crate::database::ClipboardItem,
    max_image_size_kb: u64,
) -> Result<ItemFileStatus, String> {
    use rayon::prelude::*;
    use std::path::Path;

    if item.content_type != "files" {
        return Err("Item is not a file type".to_string());
    }

    let originals = parse_file_paths(item.file_paths.as_deref());
    let resolved = resolve_item_paths(item.file_paths.as_deref(), item.file_payload.as_deref());
    let all_exist = item_files_all_exist(item.file_paths.as_deref(), item.file_payload.as_deref());

    let resolved_checks: HashMap<String, FileCheckResult> = resolved
        .par_iter()
        .map(|path| {
            let p = Path::new(path);
            let exists = p.exists();
            (
                path.clone(),
                FileCheckResult {
                    exists,
                    is_dir: exists && p.is_dir(),
                },
            )
        })
        .collect();

    let checks: HashMap<String, FileCheckResult> = originals
        .into_iter()
        .enumerate()
        .map(|(i, orig)| {
            let resolved_path = resolved.get(i).cloned().unwrap_or_else(|| orig.clone());
            let info = resolved_checks
                .get(&resolved_path)
                .cloned()
                .unwrap_or(FileCheckResult {
                    exists: false,
                    is_dir: false,
                });
            (orig, info)
        })
        .collect();

    let too_large = compute_too_large_for_preview(item, &resolved, max_image_size_kb);

    Ok(ItemFileStatus {
        all_exist,
        resolved_paths: resolved,
        checks,
        too_large,
    })
}

#[tauri::command]
pub async fn get_item_file_status(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<ItemFileStatus, String> {
    let repo = ClipboardRepository::new(&state.db);
    let item = repo
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "条目未找到".to_string())?;
    let max_image_size_kb = read_max_image_size_kb(&state.db);
    build_item_file_status(&item, max_image_size_kb)
}

#[tauri::command]
pub async fn batch_get_item_file_status(
    state: State<'_, Arc<AppState>>,
    ids: Vec<i64>,
) -> Result<HashMap<i64, ItemFileStatus>, String> {
    let repo = ClipboardRepository::new(&state.db);
    let max_image_size_kb = read_max_image_size_kb(&state.db);
    let mut out = HashMap::new();
    for id in ids {
        if let Ok(Some(item)) = repo.get_by_id(id)
            && let Ok(status) = build_item_file_status(&item, max_image_size_kb)
        {
            out.insert(id, status);
        }
    }
    Ok(out)
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
            .map_err(|e| format!("Failed to open explorer: {e}"))?;
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
        .ok_or_else(|| "条目未找到".to_string())?;

    let paths_text = if item.content_type == "files" {
        let resolved = resolve_item_paths(item.file_paths.as_deref(), item.file_payload.as_deref());
        if resolved.is_empty() {
            return Err("未找到文件路径".to_string());
        }
        resolved.join("\n")
    } else {
        return Err("Item is not a file type".to_string());
    };

    with_paused_monitor(&state, || {
        let clipboard = clipboard_rs::ClipboardContext::new()
            .map_err(|e| format!("Failed to access clipboard: {e}"))?;
        clipboard
            .set_text(paths_text)
            .map_err(|e| format!("Failed to set clipboard text: {e}"))?;

        hide_main_window_if_not_pinned(&app);

        std::thread::sleep(std::time::Duration::from_millis(50));
        super::run_simulate_paste_with_sound(&app)?;

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
        .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().to_string());

    let dest = app
        .dialog()
        .file()
        .set_title("另存为")
        .set_file_name(&file_name)
        .blocking_save_file();

    if let Some(dest_path) = dest {
        let dest_str = dest_path.to_string();
        std::fs::copy(&source_path, &dest_str).map_err(|e| format!("保存失败: {e}"))?;
        info!("File saved: {} -> {}", source_path, dest_str);
        Ok(true)
    } else {
        debug!("save_file_as: user cancelled");
        Ok(false)
    }
}

/// 获取数据目录大小明细（数据库+图片）
#[tauri::command]
pub async fn get_data_size() -> Result<DataSizeInfo, String> {
    let config = crate::config::AppConfig::load();
    let data_dir = config.get_data_dir();

    let db_size = ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"]
        .iter()
        .map(|name| std::fs::metadata(data_dir.join(name)).map_or(0, |m| m.len()))
        .sum::<u64>();

    let images_dir = data_dir.join("images");
    let (images_size, images_count) = dir_size_and_count(&images_dir);
    let staged_dir = data_dir.join("staged");
    let (staged_size, staged_count) = dir_size_and_count(&staged_dir);

    Ok(DataSizeInfo {
        db_size,
        images_size,
        images_count,
        staged_size,
        staged_count,
        total_size: db_size + images_size + staged_size,
    })
}

fn dir_size_and_count(dir: &std::path::Path) -> (u64, u64) {
    if !dir.is_dir() {
        return (0, 0);
    }
    let mut size = 0u64;
    let mut count = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                size += entry.metadata().map_or(0, |m| m.len());
                count += 1;
            }
        }
    }
    (size, count)
}

#[derive(serde::Serialize)]
pub struct DataSizeInfo {
    pub db_size: u64,
    pub images_size: u64,
    pub images_count: u64,
    pub staged_size: u64,
    pub staged_count: u64,
    pub total_size: u64,
}

/// 获取文件详情
#[tauri::command]
pub async fn get_file_details(path: String) -> Result<FileDetails, String> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&path);
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to get file metadata: {e}"))?;

    let file_type = if metadata.is_dir() {
        "folder".to_string()
    } else if metadata.is_file() {
        path.extension().map_or_else(
            || "FILE".to_string(),
            |e| e.to_string_lossy().to_uppercase(),
        )
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
