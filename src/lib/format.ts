// 剪贴板条目格式化与解析工具

import { getCurrentLocale, translate } from "@/i18n";

export function getContentTypeConfig(): Record<string, { label: string }> {
  return {
    text: { label: translate("文本") },
    html: { label: "HTML" },
    rtf: { label: "RTF" },
    image: { label: translate("图片") },
    files: { label: translate("文件") },
  };
}

export function formatTime(dateStr: string, format: "absolute" | "relative" = "absolute"): string {
  const date = new Date(dateStr);
  if (format === "relative") return formatRelativeTime(date);

  const now = new Date();
  const isToday = date.toDateString() === now.toDateString();

  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const time = `${hours}:${minutes}`;

  if (isToday) return translate("今天 {time}", { time });

  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return translate("昨天 {time}", { time });

  const month = (date.getMonth() + 1).toString().padStart(2, "0");
  const day = date.getDate().toString().padStart(2, "0");
  const locale = getCurrentLocale();
  return locale === "en-US"
    ? `${month}/${day} ${time}`
    : `${month}-${day} ${time}`;
}

function formatRelativeTime(date: Date): string {
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return translate("刚刚");
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return translate("{count} 分钟前", { count: diffMin });
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return translate("{count} 小时前", { count: diffHour });
  const diffDay = Math.floor(diffHour / 24);
  if (diffDay < 30) return translate("{count} 天前", { count: diffDay });
  const diffMonth = Math.floor(diffDay / 30);
  if (diffMonth < 12) return translate("{count} 个月前", { count: diffMonth });
  return translate("{count} 年前", { count: Math.floor(diffMonth / 12) });
}

export function formatCharCount(count: number | null): string {
  if (!count) return translate("0 字符");
  if (count >= 10000) {
    return getCurrentLocale() === "en-US"
      ? `${(count / 1000).toFixed(1)}k chars`
      : translate("{count}万 字符", { count: (count / 10000).toFixed(1) });
  }
  return translate("{count} 字符", { count: count.toLocaleString() });
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
