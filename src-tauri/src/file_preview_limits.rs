//! 文件类型剪贴板条目的图片预览大小限制（与前端 `src/lib/file-preview-limits.ts` 保持同步）

/// 与 settings `max_image_size_kb` 默认值一致（50MB）
pub const DEFAULT_MAX_IMAGE_SIZE_KB: u64 = 51200;
/// UNC/网络路径固定上限（10MB），与设置无关
pub const MAX_PREVIEW_UNC_BYTES: u64 = 10 * 1024 * 1024;
/// 设置「无限制」(0) 时本地预览硬上限，防止 WebView 读取巨型文件
pub const PREVIEW_LOCAL_SAFETY_CAP_BYTES: u64 = 100 * 1024 * 1024;

pub fn is_unc_path(path: &str) -> bool {
    path.starts_with("\\\\")
}

/// 本地路径预览上限：`max_image_size_kb` 为 0 时使用安全硬上限
pub fn local_preview_limit_bytes(max_image_size_kb: u64) -> u64 {
    if max_image_size_kb == 0 {
        PREVIEW_LOCAL_SAFETY_CAP_BYTES
    } else {
        max_image_size_kb * 1024
    }
}

pub fn preview_limit_bytes(path: &str, max_image_size_kb: u64) -> u64 {
    if is_unc_path(path) {
        MAX_PREVIEW_UNC_BYTES
    } else {
        local_preview_limit_bytes(max_image_size_kb)
    }
}

/// 判断是否应跳过图片预览（不读取文件内容）
pub fn is_too_large_for_preview(
    path: &str,
    byte_size: i64,
    max_image_size_kb: u64,
    default_unknown: bool,
) -> bool {
    if byte_size > 0 {
        return byte_size as u64 > preview_limit_bytes(path, max_image_size_kb);
    }
    if is_unc_path(path) {
        return true;
    }
    default_unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    #[test]
    fn unc_path_detection() {
        assert!(is_unc_path(r"\\server\share\a.jpg"));
        assert!(!is_unc_path(r"C:\a.jpg"));
    }

    #[test]
    fn local_limit_from_settings() {
        assert_eq!(local_preview_limit_bytes(51200), 50 * MB);
        assert_eq!(local_preview_limit_bytes(0), PREVIEW_LOCAL_SAFETY_CAP_BYTES);
    }

    #[test]
    fn too_large_with_byte_size() {
        let path = r"C:\photo.jpg";
        assert!(!is_too_large_for_preview(
            path,
            10 * MB as i64,
            51200,
            false
        ));
        assert!(is_too_large_for_preview(
            path,
            50 * MB as i64 + 1,
            51200,
            false
        ));
    }

    #[test]
    fn unc_stricter_and_unknown_defaults() {
        let unc = r"\\nas\share\photo.jpg";
        assert!(is_too_large_for_preview(unc, 11 * MB as i64, 51200, false));
        assert!(is_too_large_for_preview(unc, 0, 51200, false));
        assert!(is_too_large_for_preview(unc, 0, 51200, true));
        assert!(!is_too_large_for_preview(r"C:\photo.jpg", 0, 51200, false));
        assert!(is_too_large_for_preview(r"C:\photo.jpg", 0, 51200, true));
    }
}
