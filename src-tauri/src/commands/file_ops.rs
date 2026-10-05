use crate::clipboard::file_clipboard::{
    file_status_resources, prepare_path_text, prepare_resource_paths, resolve_item_paths,
};
use crate::database::ClipboardRepository;
use crate::file_preview_limits::{
    DEFAULT_MAX_IMAGE_SIZE_KB, is_too_large_for_preview, is_unc_path, preview_limit_bytes,
};
use crate::operation_error::{OperationError, OperationErrorCode, readable_resource};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use tracing::info;

use super::AppState;

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
    pub clipboard_usable: bool,
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

    let (originals, resolved, usable_payload) =
        file_status_resources(item.file_paths.as_deref(), item.file_payload.as_deref());

    let resolved_checks: HashMap<String, FileCheckResult> = resolved
        .par_iter()
        .map(|path| {
            let metadata = Path::new(path).metadata().ok();
            let exists = metadata.is_some();
            (
                path.clone(),
                FileCheckResult {
                    exists,
                    is_dir: metadata.is_some_and(|metadata| metadata.is_dir()),
                },
            )
        })
        .collect();
    let all_exist = if resolved.is_empty() {
        usable_payload
    } else {
        resolved.iter().all(|path| !path.is_empty())
            && resolved_checks.values().all(|check| check.exists)
    };
    let clipboard_usable = all_exist && usable_payload;

    let checks: HashMap<String, FileCheckResult> = originals
        .into_iter()
        .enumerate()
        .map(|(i, orig)| {
            let resolved_path = resolved.get(i).unwrap_or(&orig);
            let info = resolved_checks
                .get(resolved_path)
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
        clipboard_usable,
        resolved_paths: resolved,
        checks,
        too_large,
    })
}

#[tauri::command]
pub async fn get_item_file_status(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<ItemFileStatus, OperationError> {
    let item = super::clipboard::resolve_item(&state, id)?;
    let max_image_size_kb = read_max_image_size_kb(&state.db);
    build_item_file_status(&item, max_image_size_kb)
        .map_err(|error| OperationError::new(OperationErrorCode::UnsupportedContent, error))
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

fn item_resource_paths(
    item: &crate::database::ClipboardItem,
) -> Result<Vec<String>, OperationError> {
    match item.content_type.as_str() {
        "files" => prepare_resource_paths(item.file_paths.as_deref(), item.file_payload.as_deref()),
        "image" => {
            let path = item
                .image_path
                .as_ref()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| {
                    OperationError::new(OperationErrorCode::InvalidContent, "image source absent")
                })?;
            readable_resource(Path::new(path))?;
            Ok(vec![path.clone()])
        }
        _ => Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "item has no file resource",
        )),
    }
}

#[tauri::command]
pub async fn show_in_explorer(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), OperationError> {
    let item = super::clipboard::resolve_item(&state, id)?;
    let paths = item_resource_paths(&item)?;
    let path = Path::new(&paths[0]);
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    result.map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            OperationErrorCode::PermissionDenied
        } else {
            OperationErrorCode::ExplorerFailed
        };
        OperationError::new(code, format!("launch file manager: {error}"))
    })?;
    Ok(())
}

#[tauri::command]
pub async fn paste_as_path(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), OperationError> {
    let item = super::clipboard::resolve_item(&state, id)?;
    if item.content_type != "files" {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "item is not a file type",
        ));
    }
    let text = prepare_path_text(item.file_paths.as_deref(), item.file_payload.as_deref())?;
    super::clipboard::paste_plain_text_to_active_window(&state, &app, text, true)
}

fn save_source(item: &crate::database::ClipboardItem) -> Result<PathBuf, OperationError> {
    let mut paths = item_resource_paths(item)?;
    if paths.len() != 1 {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "save requires one file",
        ));
    }
    let source = PathBuf::from(paths.pop().expect("one source"));
    if !readable_resource(&source)?.is_file() {
        return Err(OperationError::new(
            OperationErrorCode::UnsupportedContent,
            "save source is not a regular file",
        ));
    }
    Ok(source)
}

pub(crate) fn copy_file_to_destination(
    source: &Path,
    destination: &Path,
) -> Result<(), OperationError> {
    readable_resource(source)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            OperationError::new(
                OperationErrorCode::InvalidDestination,
                "destination has no parent",
            )
        })?;
    let parent_metadata = std::fs::metadata(parent).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            OperationErrorCode::PermissionDenied
        } else {
            OperationErrorCode::InvalidDestination
        };
        OperationError::new(code, format!("destination parent: {error}"))
    })?;
    if !parent_metadata.is_dir() || destination.is_dir() {
        return Err(OperationError::new(
            OperationErrorCode::InvalidDestination,
            "destination is not a file path",
        ));
    }
    std::fs::copy(source, destination).map_err(|error| {
        if let Err(source_error) = readable_resource(source) {
            return source_error;
        }
        if error.kind() == std::io::ErrorKind::NotFound {
            OperationError::new(
                OperationErrorCode::InvalidDestination,
                format!("copy destination missing: {error}"),
            )
        } else {
            OperationError::io("copy to destination", error, OperationErrorCode::SaveFailed)
        }
    })?;
    Ok(())
}

#[tauri::command]
pub async fn save_file_as(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<bool, OperationError> {
    use tauri_plugin_dialog::DialogExt;
    let source = save_source(&super::clipboard::resolve_item(&state, id)?)?;
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let language = crate::database::SettingsRepository::new(&state.db)
        .get("language")
        .ok()
        .flatten()
        .unwrap_or_default();
    let title = match language.as_str() {
        "en" => "Save as",
        "zh-TW" => "另存為",
        _ => "另存为",
    };
    let Some(destination) = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(&file_name)
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let destination = destination.into_path().map_err(|error| {
        OperationError::new(
            OperationErrorCode::InvalidDestination,
            format!("dialog destination: {error}"),
        )
    })?;
    let source = save_source(&super::clipboard::resolve_item(&state, id)?)?;
    copy_file_to_destination(&source, &destination)?;
    info!(id, "file saved");
    Ok(true)
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
pub async fn get_file_details(path: String) -> Result<FileDetails, OperationError> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(&path);
    let metadata = fs::metadata(path).map_err(|error| {
        OperationError::io(
            "file details metadata",
            error,
            OperationErrorCode::ResourceUnreadable,
        )
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_copies_bytes_without_image_decoding_and_missing_source_preserves_destination() {
        let directory =
            std::env::temp_dir().join(format!("ec_save_preflight_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.png");
        let destination = directory.join("saved.png");
        std::fs::write(&source, b"undecodable image bytes").unwrap();
        copy_file_to_destination(&source, &destination).unwrap();
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"undecodable image bytes"
        );
        std::fs::remove_file(&source).unwrap();
        assert!(matches!(
            copy_file_to_destination(&source, &destination),
            Err(OperationError {
                code: OperationErrorCode::ResourceMissing,
                ..
            })
        ));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"undecodable image bytes"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_destination_does_not_change_source() {
        let directory =
            std::env::temp_dir().join(format!("ec_save_destination_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.txt");
        std::fs::write(&source, b"original").unwrap();
        let destination = directory.join("missing").join("target.txt");
        assert!(matches!(
            copy_file_to_destination(&source, &destination),
            Err(OperationError {
                code: OperationErrorCode::InvalidDestination,
                ..
            })
        ));
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        assert!(!destination.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn file_status_distinguishes_real_sources_from_clipboard_payload_support() {
        use crate::clipboard::file_clipboard::{
            FilePayload, FormatBlob, StagedFile, encode_payload,
        };
        use base64::Engine;

        let directory = std::env::temp_dir().join(format!("ec_file_status_{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source = directory.join("source.txt");
        std::fs::write(&source, b"source").unwrap();
        let source_path = source.to_string_lossy().into_owned();
        let mut item = crate::database::ClipboardItem {
            id: 1,
            content_type: "files".into(),
            text_content: None,
            html_content: None,
            rtf_content: None,
            image_path: None,
            file_paths: Some(serde_json::to_string(&vec![source_path.clone()]).unwrap()),
            file_payload: None,
            content_hash: "status".into(),
            semantic_hash: "status".into(),
            preview: None,
            byte_size: 6,
            image_width: None,
            image_height: None,
            is_pinned: false,
            is_favorite: false,
            favorite_order: 0,
            sort_order: 0,
            created_at: String::new(),
            updated_at: String::new(),
            access_count: 0,
            last_accessed_at: None,
            char_count: None,
            source_app_name: None,
            source_app_icon: None,
            group_id: None,
        };
        let normal = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(normal.all_exist && normal.clipboard_usable);

        item.file_payload = Some(r#"{"extra":[{"name":"Preferred DropEffect","b64":""}]}"#.into());
        let unsupported = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(unsupported.all_exist);
        assert!(!unsupported.clipboard_usable);
        assert_eq!(unsupported.resolved_paths, vec![source_path.clone()]);
        assert!(unsupported.checks[&source_path].exists);
        assert!(!unsupported.checks[&source_path].is_dir);
        assert!(save_source(&item).is_ok());
        assert_eq!(
            serde_json::to_value(&unsupported).unwrap()["clipboard_usable"],
            false
        );

        let missing = directory
            .join("missing-source.txt")
            .to_string_lossy()
            .into_owned();
        item.file_paths = Some(serde_json::to_string(&vec![missing.clone()]).unwrap());
        item.file_payload = Some(encode_payload(&FilePayload {
            staged: vec![StagedFile {
                original: missing.clone(),
                staged: source_path.clone(),
                size: 6,
            }],
            ..Default::default()
        }));
        let staged = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(staged.all_exist && staged.clipboard_usable);
        assert_eq!(staged.resolved_paths, vec![source_path]);
        assert!(staged.checks[&missing].exists);
        item.file_payload = None;
        let absent = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(!absent.all_exist && !absent.clipboard_usable);
        assert!(!absent.checks[&missing].exists);

        let mut descriptor = vec![0; 4 + 592];
        descriptor[..4].copy_from_slice(&1u32.to_le_bytes());
        descriptor[4 + 72..4 + 74].copy_from_slice(&('a' as u16).to_le_bytes());
        let mut virtual_payload = FilePayload {
            extra: vec![
                FormatBlob {
                    name: "FileGroupDescriptorW".into(),
                    b64: base64::engine::general_purpose::STANDARD.encode(descriptor),
                },
                FormatBlob {
                    name: "FileContents".into(),
                    b64: base64::engine::general_purpose::STANDARD.encode(b"contents"),
                },
            ],
            ..Default::default()
        };
        item.file_paths = Some("[]".into());
        item.file_payload = Some(encode_payload(&virtual_payload));
        let virtual_status = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(virtual_status.all_exist && virtual_status.clipboard_usable);
        assert!(virtual_status.resolved_paths.is_empty());

        virtual_payload.extra[1].b64.clear();
        item.file_payload = Some(encode_payload(&virtual_payload));
        let empty_virtual = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(!empty_virtual.all_exist && !empty_virtual.clipboard_usable);
        virtual_payload.extra.pop();
        item.file_payload = Some(encode_payload(&virtual_payload));
        let legacy_virtual = build_item_file_status(&item, DEFAULT_MAX_IMAGE_SIZE_KB).unwrap();
        assert!(!legacy_virtual.all_exist && !legacy_virtual.clipboard_usable);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
