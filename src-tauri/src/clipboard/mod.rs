mod dedup;
pub(crate) mod file_clipboard;
pub(crate) mod format_write;
mod handler;
pub(crate) mod merge_paste;
mod monitor;
pub(crate) mod rtf_storage;
pub mod source_app;

pub(crate) use dedup::{
    canonical_url_text, compute_semantic_hash, is_url, semantic_hash_from_text,
};
pub use handler::*;
pub use monitor::*;
use tracing::debug;

/// 从磁盘删除图片文件，失败时记录日志，返回成功删除数。
/// 同时清理历史遗留的同名 `.dib` 伴侣文件。
fn cleanup_image_files(paths: &[String]) -> usize {
    let mut deleted = 0;
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {
                debug!("Deleted image file: {}", path);
                deleted += 1;
            }
            Err(e) => {
                debug!("Failed to delete image file {}: {}", path, e);
            }
        }
        // 旧版本可能残留同名 .dib；忽略不存在的情况
        let dib_path = std::path::Path::new(path).with_extension("dib");
        match std::fs::remove_file(&dib_path) {
            Ok(()) => {
                debug!("Deleted companion DIB: {:?}", dib_path);
                deleted += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                debug!("Failed to delete companion DIB {:?}: {}", dib_path, e);
            }
        }
    }
    deleted
}

/// 从磁盘删除 staging 文件，并尝试清理空的 staging 子目录。
fn cleanup_staged_files(paths: &[String]) -> usize {
    let mut deleted = 0;
    let mut dirs: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    for path in paths {
        let p = std::path::Path::new(path);
        if let Some(parent) = p.parent() {
            dirs.insert(parent.to_path_buf());
        }
        match std::fs::remove_file(path) {
            Ok(()) => {
                debug!("Deleted staged file: {}", path);
                deleted += 1;
            }
            Err(e) => {
                debug!("Failed to delete staged file {}: {}", path, e);
            }
        }
    }
    for dir in dirs {
        let _ = std::fs::remove_dir(&dir);
    }
    deleted
}

/// 删除条目关联的图片与 staging 文件。
pub fn cleanup_deleted_assets(image_paths: &[String], file_payloads: &[String]) {
    cleanup_image_files(image_paths);
    let staged: Vec<String> = file_payloads
        .iter()
        .flat_map(|p| file_clipboard::staged_paths_from_payload(Some(p.as_str())))
        .collect();
    cleanup_staged_files(&staged);
}
