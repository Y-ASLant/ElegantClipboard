mod handler;
mod monitor;
pub mod source_app;

pub use handler::*;
pub use monitor::*;

/// 从磁盘删除图片文件，失败时记录日志，返回成功删除数。
pub fn cleanup_image_files(paths: &[String]) -> usize {
    let mut deleted = 0;
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {
                tracing::debug!("Deleted image file: {}", path);
                deleted += 1;
            }
            Err(e) => {
                tracing::debug!("Failed to delete image file {}: {}", path, e);
            }
        }
    }
    deleted
}
