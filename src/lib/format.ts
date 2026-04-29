// 剪贴板条目格式化与解析工具

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "tiff", "tif",
]);

const VIDEO_EXTENSIONS = new Set([
  "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "mpeg", "mpg",
]);

const PARSED_FILE_PATHS_CACHE_MAX = 200;
const parsedFilePathsCache = new Map<string, string[]>();

const CODE_KEYWORDS = [
  "function ", "const ", "let ", "var ", "class ", "interface ", "type ", "import ",
  "export ", "def ", "fn ", "public class", "#include", "using namespace", "console.log(",
  "=>", "</", "select ", "insert into", "update ", "delete from",
];

export const contentTypeConfig: Record<string, { label: string }> = {
  text: { label: "文本" },
  image: { label: "图片" },
  files: { label: "文件" },
  url: { label: "URL" },
  code: { label: "代码" },
  video: { label: "视频" },
};

export interface LogicalContentTypeSource {
  content_type: string;
  text_content?: string | null;
  preview?: string | null;
  file_paths?: string | null;
}

export function isUrlText(text: string): boolean {
  const value = text.trim().toLowerCase();
  return (
    (value.startsWith("http://") ||
      value.startsWith("https://") ||
      value.startsWith("ftp://") ||
      value.startsWith("file://") ||
      value.startsWith("www.")) &&
    !/\s/.test(value)
  );
}

export function isCodeText(text: string): boolean {
  const value = text.trim();
  if (!value) return false;

  const lower = value.toLowerCase();
  if (CODE_KEYWORDS.some((keyword) => lower.includes(keyword))) {
    return true;
  }

  const lines = value.split(/\r?\n/);
  const indentedLines = lines.filter((line) => /^\s{2,}\S/.test(line)).length;

  return lines.length >= 3 && indentedLines >= 1 && /[{};<>]/.test(value);
}

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

  const cached = parsedFilePathsCache.get(filePathsJson);
  if (cached) return cached;

  try {
    const paths = JSON.parse(filePathsJson);
    const normalized = Array.isArray(paths) ? paths : [];

    if (parsedFilePathsCache.size >= PARSED_FILE_PATHS_CACHE_MAX) {
      const oldestKey = parsedFilePathsCache.keys().next().value;
      if (oldestKey) {
        parsedFilePathsCache.delete(oldestKey);
      }
    }

    parsedFilePathsCache.set(filePathsJson, normalized);
    return normalized;
  } catch {
    return [];
  }
}

export function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.has(ext);
}

export function isVideoFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return VIDEO_EXTENSIONS.has(ext);
}

export function getLogicalContentType(item: LogicalContentTypeSource): keyof typeof contentTypeConfig {
  if (item.content_type === "image") {
    return "image";
  }

  if (item.content_type === "video") {
    return "video";
  }

  if (item.content_type === "files") {
    return "files";
  }

  // text_content 在列表模式下为 NULL（性能优化），使用 preview 作为 fallback
  const text = (item.text_content ?? item.preview ?? "").trim();
  if (text && isUrlText(text)) {
    return "url";
  }
  if (text && isCodeText(text)) {
    return "code";
  }
  return "text";
}