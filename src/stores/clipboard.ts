import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import debounce from "lodash.debounce";
import { create } from "zustand";
import { cancelPendingFocusRestore } from "@/hooks/useInputFocus";
import { t } from "@/i18n";
import { logError } from "@/lib/logger";
import { runClipboardOperation, reportOperationError, reportUserError, isOperationError, type ClipboardOperation, type OperationResult } from "@/lib/operation-feedback";
import { playCopySound, setupPasteSoundListeners } from "@/lib/sounds";
import { useUISettings } from "@/stores/ui-settings";

function batchResetState() {
  return { batchMode: false, selectedIds: new Set<number>(), lastSelectedIndex: -1 };
}

export interface ClipboardItem {
  id: number;
  content_type: "text" | "image" | "html" | "rtf" | "files" | "url";
  text_content: string | null;
  html_content: string | null;
  rtf_content: string | null;
  image_path: string | null;
  file_paths: string | null;
  content_hash: string;
  preview: string | null;
  byte_size: number;
  image_width: number | null;
  image_height: number | null;
  is_pinned: boolean;
  is_favorite: boolean;
  favorite_order: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
  access_count: number;
  last_accessed_at: string | null;
  char_count: number | null;
  source_app_name: string | null;
  source_app_icon: string | null;
  /** 所属自定义分组 ID（null = 默认分组） */
  group_id: number | null;
}

interface ClipboardState {
  items: ClipboardItem[];
  isLoading: boolean;
  searchQuery: string;
  selectedGroup: string | null;
  /** 当前选中的自定义分组 id，可与类型或收藏筛选组合 */
  selectedGroupId: number | null;
  /** 当前键盘高亮索引（-1 表示无） */
  activeIndex: number;
  /** 单调计数器，丢弃过期请求 */
  _fetchId: number;
  /** 视图重置计数（滚动到顶部等） */
  _resetToken: number;

  // 操作
  fetchItems: (options?: {
    search?: string;
    content_type?: string;
    limit?: number;
    offset?: number;
    feedbackOperation?: ClipboardOperation;
  }) => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSelectedGroup: (group: string | null) => void;
  setSelectedGroupId: (groupId: number | null) => void;
  setActiveIndex: (index: number) => void;
  togglePin: (id: number) => Promise<OperationResult<boolean>>;
  toggleFavorite: (id: number) => Promise<OperationResult<boolean>>;
  moveItem: (fromId: number, toId: number) => Promise<OperationResult>;
  moveFavoriteItem: (fromId: number, toId: number) => Promise<OperationResult>;
  deleteItem: (id: number) => Promise<OperationResult>;
  copyToClipboard: (id: number) => Promise<OperationResult>;
  pasteContent: (id: number) => Promise<OperationResult>;
  pasteAsPlainText: (id: number) => Promise<OperationResult>;
  /** 清空当前分组历史，成功时返回删除条数 */
  clearHistory: (contentType?: string | null) => Promise<OperationResult<number>>;
  refresh: (feedbackOperation?: ClipboardOperation) => Promise<void>;
  /** 重置视图：清除搜索、类型筛选，滚动到顶部，刷新 */
  resetView: () => Promise<void>;
  setupListener: () => Promise<() => void>;

  // 批量选择
  batchMode: boolean;
  selectedIds: Set<number>;
  lastSelectedIndex: number;
  setBatchMode: (enabled: boolean) => void;
  toggleSelect: (id: number, index: number, shiftKey: boolean) => void;
  selectAll: () => void;
  deselectAll: () => void;
  batchDelete: () => Promise<OperationResult>;
}

async function doPaste(
  get: () => ClipboardState,
  id: number,
  operation: "paste" | "pastePlain",
): Promise<OperationResult> {
  cancelPendingFocusRestore();
  const { pasteCloseWindow, pasteMoveToTop } = useUISettings.getState();
  const result = await runClipboardOperation(operation, { id, closeWindow: pasteCloseWindow });
  if (result.status === "success" && pasteMoveToTop) {
    void runClipboardOperation("reorder", { id }).then((reordered) => {
      if (reordered.status === "success") return get().refresh("reorder");
    }).catch((error) => reportOperationError("reorder", error));
  }
  return result;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  isLoading: false,
  searchQuery: "",
  selectedGroup: null,
  selectedGroupId: null,
  activeIndex: -1,
  _fetchId: 0,
  _resetToken: 0,

  fetchItems: async (options = {}) => {
    const state = get();
    const fetchId = state._fetchId + 1;
    set({ isLoading: true, _fetchId: fetchId });
    try {
      const group = options.content_type ?? state.selectedGroup;
      const isFavoritesView = group === "__favorites__";
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: options.search ?? (state.searchQuery || null),
        contentType: isFavoritesView ? null : group,
        pinnedOnly: false,
        favoriteOnly: isFavoritesView,
        groupId: state.selectedGroupId,
        limit: options.limit,
        offset: options.offset ?? 0,
      });
      if (get()._fetchId === fetchId) {
        set({ items, isLoading: false, activeIndex: -1 });
      }
    } catch (error) {
      if (get()._fetchId === fetchId) {
        if (options.feedbackOperation) reportUserError(t("operationFeedback.refreshAfterSuccess"), error, `Refresh after '${options.feedbackOperation}' failed`);
        else logError("Failed to fetch items:", error);
        set({ isLoading: false });
      }
    }
  },

  setSearchQuery: (query: string) => {
    set((state) => ({
      searchQuery: query,
      _fetchId: state._fetchId + 1,
      isLoading: false,
    }));
    // 仅更新查询状态，防抖在 App.tsx 中处理
  },

  setSelectedGroup: (group: string | null) => {
    set((state) => ({
      selectedGroup: group,
      _fetchId: state._fetchId + 1,
      ...batchResetState(),
    }));
    get().fetchItems();
  },

  setSelectedGroupId: (groupId: number | null) => {
    set((state) => ({
      selectedGroupId: groupId,
      _fetchId: state._fetchId + 1,
      ...batchResetState(),
    }));
    invoke("set_active_group", { groupId }).catch((error) => {
      logError("Failed to persist active group:", error);
    });
    get().fetchItems();
  },

  setActiveIndex: (index: number) => {
    set({ activeIndex: index });
  },

  togglePin: async (id) => {
    const result = await runClipboardOperation<boolean>("pin", { id });
    if (result.status === "success") await get().refresh("pin");
    return result;
  },

  toggleFavorite: async (id) => {
    const result = await runClipboardOperation<boolean>("favorite", { id });
    if (result.status === "success") {
      if (!result.value && get().selectedGroup === "__favorites__") {
        await get().refresh("favorite");
      } else {
        set((state) => ({
          items: state.items.map((item) =>
            item.id === id ? { ...item, is_favorite: result.value } : item
          ),
        }));
      }
    }
    return result;
  },

  moveItem: async (fromId, toId) => {
    const result = await runClipboardOperation("move", { fromId, toId });
    if (result.status === "success") await get().refresh("move");
    return result;
  },

  moveFavoriteItem: async (fromId, toId) => {
    const result = await runClipboardOperation("moveFavorite", { fromId, toId });
    if (result.status === "success") await get().refresh("moveFavorite");
    return result;
  },

  deleteItem: async (id) => {
    const result = await runClipboardOperation("delete", { id });
    if (result.status === "success") {
      set((state) => ({ items: state.items.filter((item) => item.id !== id) }));
    }
    return result;
  },

  copyToClipboard: (id) => runClipboardOperation("copy", { id }),

  pasteContent: (id) => doPaste(get, id, "paste"),

  pasteAsPlainText: (id) => doPaste(get, id, "pastePlain"),

  // contentType=null 时后端 Option<String> 为 None，清除所有类型（正确行为）
  clearHistory: async (contentType = null) => {
    const result = await runClipboardOperation<number>("clear", {
      groupId: get().selectedGroupId,
      contentType,
    });
    if (result.status === "success") await get().refresh("clear");
    return result;
  },

  refresh: async (feedbackOperation) => {
    await get().fetchItems({ feedbackOperation });
  },

  resetView: async () => {
    // 仅重置搜索和类型筛选，保留分组选择
    set((state) => ({
      searchQuery: "",
      selectedGroup: null,
      ...batchResetState(),
      _resetToken: state._resetToken + 1,
      _fetchId: state._fetchId + 1,
    }));
    await get().fetchItems({ search: "" });
  },

  setupListener: async () => {
    const unlistenPasteSound = await setupPasteSoundListeners();
    let disposed = false;
    let refreshing = false;
    let refreshPending = false;

    // 合并连续捕获并定期刷新权威轻量列表，同时同步后端淘汰的记录
    const debouncedCaptureUpdate = debounce(async () => {
      if (refreshing) {
        refreshPending = true;
        return;
      }
      refreshing = true;
      refreshPending = false;
      try {
        await get().fetchItems();
        if (!disposed) playCopySound("after_success");
      } finally {
        refreshing = false;
        if (refreshPending && !disposed) void debouncedCaptureUpdate();
      }
    }, 50, { leading: false, trailing: true, maxWait: 250 });

    const unlisten = await listen<number>("clipboard-updated", (event) => {
      const id = event.payload;
      if (disposed || typeof id !== "number" || !Number.isFinite(id)) {
        return;
      }
      playCopySound("immediate");
      void debouncedCaptureUpdate();
    }).catch((error) => {
      unlistenPasteSound();
      throw error;
    });
    const unlistenFailure = await listen<unknown>("clipboard-operation-failed", ({ payload }) => {
      if (disposed || typeof payload !== "object" || payload === null ||
          !("operation" in payload) || payload.operation !== "paste" ||
          !("error" in payload) || !isOperationError(payload.error)) return;
      reportOperationError("paste", payload.error);
    }).catch((error) => {
      disposed = true;
      debouncedCaptureUpdate.cancel();
      unlistenPasteSound();
      unlisten();
      throw error;
    });
    return () => {
      disposed = true;
      debouncedCaptureUpdate.cancel();
      unlistenPasteSound();
      unlisten();
      unlistenFailure();
    };
  },

  // 批量选择
  batchMode: false,
  selectedIds: new Set<number>(),
  lastSelectedIndex: -1,

  setBatchMode: (enabled) => {
    set({ ...batchResetState(), batchMode: enabled });
  },

  toggleSelect: (id, index, shiftKey) => {
    const { selectedIds, lastSelectedIndex, items } = get();
    const next = new Set(selectedIds);

    if (shiftKey && lastSelectedIndex >= 0) {
      const from = Math.min(lastSelectedIndex, index);
      const to = Math.max(lastSelectedIndex, index);
      for (let i = from; i <= to; i++) {
        if (items[i]) next.add(items[i].id);
      }
    } else {
      if (next.has(id)) next.delete(id);
      else next.add(id);
    }
    set({ selectedIds: next, lastSelectedIndex: index });
  },

  selectAll: () => {
    const ids = new Set(get().items.map((item) => item.id));
    set({ selectedIds: ids });
  },

  deselectAll: () => {
    set({ selectedIds: new Set() });
  },

  batchDelete: async () => {
    const { selectedIds } = get();
    if (selectedIds.size === 0) return { status: "cancelled" };
    const result = await runClipboardOperation("batchDelete", { ids: Array.from(selectedIds) });
    if (result.status === "success") {
      set({ ...batchResetState() });
      await get().refresh("batchDelete");
    }
    return result;
  },
}));

