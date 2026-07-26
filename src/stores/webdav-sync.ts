import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { create } from "zustand";
import { t } from "@/i18n";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";
import { loadUISettingsFromBackend } from "@/stores/ui-settings";

export type SyncStatusType = "success" | "error" | "info";

type WebdavManualSyncResponse = {
  message: string;
  pending_media_workers: number;
};

type WebDAVSyncState = {
  testing: boolean;
  syncing: boolean;
  statusMsg: string;
  statusType: SyncStatusType;
  pendingMediaWorkers: number;
  setStatusMsg: (msg: string | ((prev: string) => string)) => void;
  setStatusType: (type: SyncStatusType) => void;
  handleTestConnection: () => Promise<void>;
  handleUpload: () => Promise<void>;
  handleDownload: () => Promise<void>;
};

const MEDIA_PENDING_UPLOAD = "媒体文件正在后台上传";
const MEDIA_PENDING_DOWNLOAD = "媒体文件正在后台下载";

function stripMediaPendingLines(msg: string): string {
  return msg
    .split("\n")
    .filter(
      (line) =>
        !line.includes(MEDIA_PENDING_UPLOAD)
        && !line.includes(MEDIA_PENDING_DOWNLOAD),
    )
    .join("\n");
}

function localizeWebDAVError(error: unknown): string {
  if (typeof error !== "string") return String(error);
  if (error === "WEBDAV:SYNC_IN_PROGRESS") {
    return t("settings.sync.errors.syncInProgress");
  }
  return error;
}

export const useWebDAVSyncStore = create<WebDAVSyncState>((set, get) => ({
  testing: false,
  syncing: false,
  statusMsg: "",
  statusType: "info",
  pendingMediaWorkers: 0,

  setStatusMsg: (msg) =>
    set({
      statusMsg: typeof msg === "function" ? msg(get().statusMsg) : msg,
    }),

  setStatusType: (statusType) => set({ statusType }),

  handleTestConnection: async () => {
    set({ testing: true });
    try {
      const msg = await invoke<string>("webdav_test_connection");
      set({ statusMsg: msg, statusType: "success" });
    } catch (error) {
      set({
        statusMsg: localizeWebDAVError(error),
        statusType: "error",
      });
    } finally {
      set({ testing: false });
    }
  },

  handleUpload: async () => {
    if (get().syncing) return;
    set({ syncing: true, pendingMediaWorkers: 0 });
    try {
      const res = await invoke<WebdavManualSyncResponse>("webdav_upload");
      set({
        statusMsg: res.message,
        statusType: "success",
        pendingMediaWorkers: res.pending_media_workers,
      });
    } catch (error) {
      set({
        statusMsg: localizeWebDAVError(error),
        statusType: "error",
        pendingMediaWorkers: 0,
      });
    } finally {
      set({ syncing: false });
    }
  },

  handleDownload: async () => {
    if (get().syncing) return;
    set({ syncing: true, pendingMediaWorkers: 0 });
    let msg = "";
    let pendingMediaWorkers = 0;
    try {
      const res = await invoke<WebdavManualSyncResponse>("webdav_download");
      msg = res.message;
      pendingMediaWorkers = res.pending_media_workers;
      set({ statusMsg: msg, statusType: "success", pendingMediaWorkers });
    } catch (error) {
      set({
        statusMsg: localizeWebDAVError(error),
        statusType: "error",
        pendingMediaWorkers: 0,
      });
      return;
    } finally {
      set({ syncing: false });
    }

    try {
      await loadUISettingsFromBackend();
      await useClipboardStore.getState().refresh();
    } catch (error) {
      logError("WebDAV 下载后刷新本地状态失败:", error);
    }
  },
}));

const lastSyncListeners = new Set<() => void>();

/** SyncTab 挂载时注册，用于刷新最近同步时间 */
export function onWebDAVLastSyncUpdated(listener: () => void) {
  lastSyncListeners.add(listener);
  return () => {
    lastSyncListeners.delete(listener);
  };
}

let listenersInitialized = false;
let unlistenFns: UnlistenFn[] = [];

/** 设置窗口级初始化：切换 Tab 后仍能收到媒体同步完成等事件 */
export async function initWebDAVSyncListeners() {
  if (listenersInitialized) return;

  try {
    unlistenFns.push(
      await listen("webdav-media-ready", async () => {
        try {
          await useClipboardStore.getState().refresh();
        } catch (error) {
          logError("WebDAV 媒体就绪后刷新列表失败:", error);
        }
      }),
    );
    unlistenFns.push(
      await listen<string>("media-sync-done", (event) => {
        const { statusMsg, pendingMediaWorkers } =
          useWebDAVSyncStore.getState();
        const pendingLeft = Math.max(0, pendingMediaWorkers - 1);
        let nextMsg = statusMsg
          ? `${statusMsg}\n${event.payload}`
          : event.payload;
        if (pendingLeft === 0) {
          nextMsg = stripMediaPendingLines(nextMsg);
        }
        useWebDAVSyncStore.setState({
          statusMsg: nextMsg,
          pendingMediaWorkers: pendingLeft,
          statusType: "success",
        });
      }),
    );
    unlistenFns.push(
      await listen<string>("webdav-last-sync-updated", () => {
        for (const listener of lastSyncListeners) {
          listener();
        }
      }),
    );
    listenersInitialized = true;
  } catch {
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns = [];
    // 非 Tauri 环境（单元测试）
  }
}

/** 测试用：重置 store 与监听注册标记 */
export function resetWebDAVSyncStoreForTests() {
  for (const unlisten of unlistenFns) {
    unlisten();
  }
  unlistenFns = [];
  listenersInitialized = false;
  lastSyncListeners.clear();
  useWebDAVSyncStore.setState({
    testing: false,
    syncing: false,
    statusMsg: "",
    statusType: "info",
    pendingMediaWorkers: 0,
  });
}
