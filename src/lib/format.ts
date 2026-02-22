// Pure formatting and parsing utilities for clipboard items

export const contentTypeConfig: Record<string, { label: string }> = {
  text: { label: "文本" },
  html: { label: "HTML" },
  rtf: { label: "RTF" },
  image: { label: "图片" },
  files: { label: "文件" },
};

export function formatTime(dateStr: string, format: "absolute" | "relative" = "absolute"): string {
  const date = new Date(dateStr);
  if (format === "relative") return formatRelativeTime(date);

  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();

  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const time = `${hours}:${minutes}`;

  if (isToday) return `今天 ${time}`;

  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return `昨天 ${time}`;

  const month = (date.getMonth() + 1).toString().padStart(2, "0");
  const day = date.getDate().toString().padStart(2, "0");
  return `${month}-${day} ${time}`;
}

function formatRelativeTime(date: Date): string {
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return "刚刚";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} 分钟前`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour} 小时前`;
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 30) return `${diffDay} 天前`;
  const diffMonth = Math.floor(diffDay / 30);
  if (diffMonth < 12) return `${diffMonth} 个月前`;
  return `${Math.floor(diffMonth / 12)} 年前`;
}

export function formatCharCount(count: number | null): string {
  if (!count) return "0 字符";
  return count >= 10000
    ? `${(count / 10000).toFixed(1)}万 字符`
    : `${count.toLocaleString()} 字符`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function getFileNameFromPath(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || path;
}

export function parseFilePaths(filePathsJson: string | null): string[] {
  if (!filePathsJson) return [];
  try {
    const paths = JSON.parse(filePathsJson);
    return Array.isArray(paths) ? paths : [];
  } catch {
    return [];
  }
}

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tiff", "tif",
]);

export function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.has(ext);
}
