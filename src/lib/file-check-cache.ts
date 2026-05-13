import { invoke } from "@tauri-apps/api/core";

export type FileExistInfo = { exists: boolean; is_dir: boolean };

const FILE_CHECK_CACHE_TTL = 5000;
const fileCheckCache = new Map<string, { data: Record<string, FileExistInfo>; ts: number }>();

function fileCheckCacheKey(paths: string[]): string {
  // 排序后生成 key，确保相同文件集合的不同排列顺序命中同一缓存
  const sorted = [...paths].sort();
  return sorted.join("\0");
}

export async function cachedCheckFilesExist(
  paths: string[],
): Promise<Record<string, FileExistInfo>> {
  const key = fileCheckCacheKey(paths);
  const cached = fileCheckCache.get(key);
  if (cached && Date.now() - cached.ts < FILE_CHECK_CACHE_TTL) {
    return cached.data;
  }

  const data = await invoke<Record<string, FileExistInfo>>(
    "check_files_exist",
    { paths },
  );
  fileCheckCache.set(key, { data, ts: Date.now() });

  if (fileCheckCache.size > 100) {
    const oldest = fileCheckCache.keys().next().value;
    if (oldest !== undefined) fileCheckCache.delete(oldest);
  }
  return data;
}