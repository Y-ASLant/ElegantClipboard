import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { parseFilePaths } from "@/lib/format";
import { logError } from "@/lib/logger";
import type { ClipboardItem } from "@/stores/clipboard";

export type ResourceAvailability = "unknown" | "checking" | "available" | "unavailable";

interface FileCheckResult {
  exists: boolean;
  is_dir: boolean;
}

export interface ItemFileStatus {
  all_exist: boolean;
  clipboard_usable: boolean;
  resolved_paths: string[];
  checks: Record<string, FileCheckResult>;
  too_large?: boolean;
}

export interface ItemResourceStatus {
  availability: ResourceAvailability;
  isChecking: boolean;
  clipboardUsable: boolean;
  paths: string[];
  originalPaths: string[];
  tooLarge: boolean;
  refresh: () => void;
}

interface PendingItemCheck {
  id: number;
  resolve: (status: ItemFileStatus) => void;
  reject: (error: unknown) => void;
}

interface PendingImageCheck {
  path: string;
  resolve: (status: FileCheckResult) => void;
  reject: (error: unknown) => void;
}

const BATCH_DELAY_MS = 50;
let pendingItemChecks: PendingItemCheck[] = [];
let itemBatchTimer: number | null = null;
let pendingImageChecks: PendingImageCheck[] = [];
let imageBatchTimer: number | null = null;

type ResourceSnapshot = Pick<ItemResourceStatus, "availability" | "paths" | "tooLarge" | "clipboardUsable">;
interface CachedResource {
  snapshot?: ResourceSnapshot;
}
// Display-only, bounded session memory. Every mount still checks native resources.
const resourceCache = new Map<string, CachedResource>();
const MAX_CACHED_RESOURCES = 512;

function batchGetItemFileStatus(id: number): Promise<ItemFileStatus> {
  return new Promise((resolve, reject) => {
    pendingItemChecks.push({ id, resolve, reject });
    if (itemBatchTimer !== null) return;
    itemBatchTimer = window.setTimeout(() => {
      itemBatchTimer = null;
      const batch = pendingItemChecks;
      pendingItemChecks = [];
      const ids = [...new Set(batch.map((check) => check.id))];
      void invoke<Record<string, ItemFileStatus>>("batch_get_item_file_status", { ids })
        .then((results) => {
          for (const check of batch) {
            const status = results[String(check.id)];
            if (status) check.resolve(status);
            else check.reject(new Error(`No file status for item ${check.id}`));
          }
        })
        .catch((error: unknown) => {
          for (const check of batch) check.reject(error);
        });
    }, BATCH_DELAY_MS);
  });
}

function batchCheckImagePath(path: string): Promise<FileCheckResult> {
  return new Promise((resolve, reject) => {
    pendingImageChecks.push({ path, resolve, reject });
    if (imageBatchTimer !== null) return;
    imageBatchTimer = window.setTimeout(() => {
      imageBatchTimer = null;
      const batch = pendingImageChecks;
      pendingImageChecks = [];
      const paths = [...new Set(batch.map((check) => check.path))];
      void invoke<Record<string, FileCheckResult>>("check_files_exist", { paths })
        .then((results) => {
          for (const check of batch) {
            const status = results[check.path];
            if (status) check.resolve(status);
            else check.reject(new Error(`No file status for image ${check.path}`));
          }
        })
        .catch((error: unknown) => {
          for (const check of batch) check.reject(error);
        });
    }, BATCH_DELAY_MS);
  });
}

type ResourceItem = Pick<ClipboardItem, "id" | "content_type" | "file_paths" | "image_path" | "byte_size" | "content_hash">;

export function useItemResourceStatus(item: ResourceItem, enabled = true): ItemResourceStatus {
  const source = useMemo(() => {
    const originalPaths = item.content_type === "files"
      ? parseFilePaths(item.file_paths)
      : item.content_type === "image" && item.image_path ? [item.image_path] : [];
    const needsCheck = item.content_type === "files" || item.content_type === "image";
    const availability: ResourceAvailability = !needsCheck
      ? "available"
      : !enabled ? "unknown"
        : item.content_type === "image" && !item.image_path ? "unavailable" : "checking";
    const key = JSON.stringify([item.id, item.content_type, item.file_paths, item.image_path, item.byte_size, item.content_hash]);
    return { id: item.id, key, contentType: item.content_type, originalPaths, needsCheck, availability };
  }, [item.id, item.content_type, item.file_paths, item.image_path, item.byte_size, item.content_hash, enabled]);
  const initialSnapshot = useMemo(() => ({
    source,
    availability: source.availability,
    paths: source.originalPaths,
    tooLarge: false,
    clipboardUsable: !source.needsCheck,
  }), [source]);
  const mountSnapshot = useMemo(() => ({
    ...initialSnapshot,
    ...(enabled && source.needsCheck ? resourceCache.get(source.key)?.snapshot : undefined),
    isChecking: source.availability === "checking",
  }), [initialSnapshot, source, enabled]);
  const [snapshot, setSnapshot] = useState(mountSnapshot);
  const activeSource = useRef<typeof source | null>(null);
  const requestGeneration = useRef(0);

  const refresh = useCallback(() => {
    if (activeSource.current !== source || !enabled || !source.needsCheck) return;
    const generation = ++requestGeneration.current;
    const isChecking = source.availability === "checking";
    // Preserve presentation, but revoke operation authorization until revalidated.
    setSnapshot((previous) => ({
      ...(previous.source === source ? previous : mountSnapshot),
      isChecking,
    }));
    if (!isChecking) return;
    const entry: CachedResource = { snapshot: resourceCache.get(source.key)?.snapshot };
    resourceCache.delete(source.key);
    resourceCache.set(source.key, entry);
    if (resourceCache.size > MAX_CACHED_RESOURCES) {
      const oldestKey = resourceCache.keys().next().value;
      if (oldestKey !== undefined) resourceCache.delete(oldestKey);
    }

    const request = source.contentType === "files"
      ? batchGetItemFileStatus(source.id).then((status) => ({
        availability: status.all_exist ? "available" as const : "unavailable" as const,
        paths: status.resolved_paths,
        tooLarge: status.too_large ?? false,
        clipboardUsable: status.clipboard_usable,
      }))
      : batchCheckImagePath(source.originalPaths[0]).then((status) => ({
        availability: status.exists ? "available" as const : "unavailable" as const,
        paths: source.originalPaths,
        tooLarge: false,
        clipboardUsable: status.exists,
      }));
    void request
      .then((result) => {
        if (activeSource.current === source && requestGeneration.current === generation) {
          if (resourceCache.get(source.key) === entry) entry.snapshot = result;
          setSnapshot({ source, ...result, isChecking: false });
        }
      })
      .catch((error: unknown) => {
        if (activeSource.current === source && requestGeneration.current === generation) {
          if (resourceCache.get(source.key) === entry) resourceCache.delete(source.key);
          setSnapshot({ ...initialSnapshot, availability: "unknown", isChecking: false });
          logError("Failed to check item resource status:", error);
        }
      });
  }, [source, initialSnapshot, mountSnapshot, enabled]);

  useEffect(() => {
    activeSource.current = source;
    refresh();
    const unlisten = enabled && source.needsCheck
      ? listen("window-shown", refresh).catch((error: unknown) => {
        logError("Failed to listen for resource status refresh:", error);
        return null;
      })
      : null;
    return () => {
      activeSource.current = null;
      ++requestGeneration.current;
      // listen may resolve after unmount; still dispose its subscription.
      if (unlisten) {
        void unlisten.then((dispose) => dispose?.()).catch((error: unknown) => {
          logError("Failed to dispose resource status listener:", error);
        });
      }
    };
  }, [source, enabled, refresh]);

  // Source changes must never expose the previous item's paths or authorization,
  // even during the render before its new effect has started.
  const current = snapshot.source === source ? snapshot : mountSnapshot;
  return {
    availability: current.availability,
    isChecking: current.isChecking,
    clipboardUsable: !current.isChecking && current.clipboardUsable,
    paths: current.paths,
    originalPaths: source.originalPaths,
    tooLarge: current.tooLarge,
    refresh,
  };
}
