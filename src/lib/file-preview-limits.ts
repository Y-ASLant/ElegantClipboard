/**
 * 文件类型剪贴板条目的图片预览大小限制
 * 与 src-tauri/src/file_preview_limits.rs 保持同步
 */

/** 与 settings `max_image_size_kb` 默认值一致（50MB） */
export const DEFAULT_MAX_IMAGE_SIZE_KB = 51200;
/** UNC/网络路径固定上限（10MB） */
export const MAX_PREVIEW_UNC_BYTES = 10 * 1024 * 1024;
/** 设置「无限制」(0) 时本地预览硬上限 */
export const PREVIEW_LOCAL_SAFETY_CAP_BYTES = 100 * 1024 * 1024;

let maxLocalPreviewBytes = DEFAULT_MAX_IMAGE_SIZE_KB * 1024;

/** 应用启动或设置变更时同步本地预览上限 */
export function setMaxLocalPreviewBytesFromKb(maxImageSizeKb: number) {
  maxLocalPreviewBytes =
    maxImageSizeKb > 0
      ? maxImageSizeKb * 1024
      : PREVIEW_LOCAL_SAFETY_CAP_BYTES;
}

/** 从后端 settings 同步预览上限（启动与窗口显示时调用） */
export async function syncFilePreviewLimitsFromSettings(): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const raw = await invoke<string | null>("get_setting", {
      key: "max_image_size_kb",
    });
    const kb = raw ? parseInt(raw, 10) : DEFAULT_MAX_IMAGE_SIZE_KB;
    setMaxLocalPreviewBytesFromKb(
      Number.isFinite(kb) ? kb : DEFAULT_MAX_IMAGE_SIZE_KB,
    );
  } catch {
    setMaxLocalPreviewBytesFromKb(DEFAULT_MAX_IMAGE_SIZE_KB);
  }
}

export function getMaxLocalPreviewBytes() {
  return maxLocalPreviewBytes;
}

export function isUncPath(path: string): boolean {
  return path.startsWith("\\\\");
}

export function previewLimitBytes(filePath: string): number {
  return isUncPath(filePath) ? MAX_PREVIEW_UNC_BYTES : maxLocalPreviewBytes;
}

/**
 * 判断是否应跳过图片预览（不 convertFileSrc 读文件）
 * @param defaultForUnknown byteSize 无效时本地路径的默认值（true=安全优先）
 */
export function isFileTooLargeForPreview(
  filePath: string,
  byteSize?: number,
  defaultForUnknown = false,
): boolean {
  if (byteSize !== undefined && byteSize > 0) {
    return byteSize > previewLimitBytes(filePath);
  }
  if (isUncPath(filePath)) {
    return true;
  }
  return defaultForUnknown;
}

/** 单文件图片是否应跳过预览（合并后端标记与前端大小检查） */
export function shouldSkipFileImagePreview(
  filePath: string,
  byteSize: number | undefined,
  backendTooLarge: boolean,
): boolean {
  return (
    backendTooLarge ||
    isFileTooLargeForPreview(filePath, byteSize, true)
  );
}

/** byte_size 已确认超限时，无需 batch IPC 查 exists/metadata */
export function isKnownTooLargeForPreview(
  filePath: string,
  byteSize: number,
): boolean {
  return byteSize > 0 && isFileTooLargeForPreview(filePath, byteSize, true);
}
