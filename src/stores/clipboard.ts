import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { cancelPendingFocusRestore } from "@/hooks/useInputFocus";
import { LOGICAL_TYPE_BACKEND_MAP } from "@/lib/constants";
import { getLogicalContentType } from "@/lib/format";
import { logError } from "@/lib/logger";
import { useUISettings } from "@/stores/ui-settings";

export interface ClipboardItem {
  id: number;
  content_type: "text" | "image" | "html" | "rtf" | "files" | "video";
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
  sort_order: number;
  created_at: string;
  updated_at: string;
  access_count: number;
  last_accessed_at: string | null;
  char_count: number | null;
  source_app_name: string | null;
  source_app_icon: string | null;
  /** 所有文件是否存在（仅 files 类型，查询时计算） */
  files_valid?: boolean;
}

interface ClipboardState {
  items: ClipboardItem[];
  isLoading: boolean;
  searchQuery: string;
  selectedCategory: string | null;
  /** 当前选中的标签 id */
  selectedTagId: number | null;
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
  }) => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSelectedCategory: (category: string | null) => void;
  setSelectedTagId: (tagId: number | null) => void;
  setActiveIndex: (index: number) => void;
  togglePin: (id: number) => Promise<void>;
  toggleFavorite: (id: number) => Promise<void>;
  moveItem: (fromId: number, toId: number) => Promise<void>;
  deleteItem: (id: number) => Promise<void>;
  copyToClipboard: (id: number) => Promise<void>;
  pasteContent: (id: number) => Promise<void>;
  pasteAsPlainText: (id: number) => Promise<void>;
  clearHistory: (contentType?: string | null) => Promise<void>;
  refresh: () => Promise<void>;
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
  batchDelete: () => Promise<void>;
}

async function doPaste(
  get: () => ClipboardState,
  id: number,
  command: "paste_content" | "paste_content_as_plain",
) {
  try {
    cancelPendingFocusRestore();
    const { pasteCloseWindow, pasteMoveToTop } = useUISettings.getState();
    await invoke(command, { id, closeWindow: pasteCloseWindow });
    if (pasteMoveToTop) {
      invoke("bump_item_to_top", { id }).then(() => get().refresh()).catch((e) => logError("Failed to bump item to top:", e));
    }
  } catch (error) {
    logError(`Failed to ${command}:`, error);
  }
}

const EMPTY_SET: ReadonlySet<number> = new Set<number>();

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  isLoading: false,
  searchQuery: "",
  selectedCategory: null,
  selectedTagId: null,
  activeIndex: -1,
  _fetchId: 0,
  _resetToken: 0,

  fetchItems: async (options = {}) => {
    const state = get();
    const fetchId = state._fetchId + 1;
    set({ isLoading: true, _fetchId: fetchId });
    try {
      const category = options.content_type ?? state.selectedCategory;
      const isFavoritesView = category === "__favorites__";

      // 逻辑类型映射：将前端逻辑分类转为后端 contentType
      const logicalMapping = category ? LOGICAL_TYPE_BACKEND_MAP[category] : undefined;
      const backendContentType = isFavoritesView
        ? null
        : logicalMapping
          ? logicalMapping.backendType
          : category;

      // 仅在主页视图（非收藏、非标签视图）下应用隐藏设置
      const isMainView = !isFavoritesView && !state.selectedTagId;
      const { hideFavoritedFromMain, hideTaggedFromMain } = useUISettings.getState();

      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: options.search ?? (state.searchQuery || null),
        contentType: backendContentType,
        pinnedOnly: false,
        favoriteOnly: isFavoritesView,
        tagId: state.selectedTagId,
        excludeFavorited: isMainView && hideFavoritedFromMain,
        excludeTagged: isMainView && hideTaggedFromMain,
        limit: options.limit ?? null,
        offset: options.offset ?? 0,
      });

      // 前端二次过滤：逻辑类型精确筛选
      const filtered = logicalMapping
        ? items.filter((item) => getLogicalContentType(item) === logicalMapping.logicalType)
        : items;

      if (get()._fetchId === fetchId) {
        set({ items: filtered, isLoading: false, activeIndex: -1 });
      }
    } catch (error) {
      if (get()._fetchId === fetchId) {
        logError("Failed to fetch items:", error);
        set({ isLoading: false });
      }
    }
  },

  setSearchQuery: (query: string) => {
    set({ searchQuery: query });
    // 仅更新查询状态，防抖在 App.tsx 中处理
  },

  setSelectedCategory: (category: string | null) => {
    set({ selectedCategory: category, batchMode: false, selectedIds: EMPTY_SET as Set<number>, lastSelectedIndex: -1 });
    get().fetchItems();
  },

  setSelectedTagId: (tagId: number | null) => {
    set({ selectedTagId: tagId, batchMode: false, selectedIds: EMPTY_SET as Set<number>, lastSelectedIndex: -1 });
    get().fetchItems();
  },

  setActiveIndex: (index: number) => {
    set({ activeIndex: index });
  },

  togglePin: async (id: number) => {
    try {
      await invoke<boolean>("toggle_pin", { id });
      // 刷新以获取正确排序（置顶优先）
      await get().refresh();
    } catch (error) {
      logError("Failed to toggle pin:", error);
    }
  },

  toggleFavorite: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_favorite", { id });
      // 在收藏视图中取消收藏时，需要刷新列表以移除该条目
      // 在主页视图中收藏且开启了"收藏后从主页隐藏"时，也需要刷新
      const isFavoritesView = get().selectedCategory === "__favorites__";
      const shouldRefresh =
        (!newState && isFavoritesView) ||
        (newState && !isFavoritesView && !get().selectedTagId && useUISettings.getState().hideFavoritedFromMain);
      if (shouldRefresh) {
        await get().refresh();
      } else {
        set((state) => ({
          items: state.items.map((item) =>
            item.id === id ? { ...item, is_favorite: newState } : item
          ),
        }));
      }
    } catch (error) {
      logError("Failed to toggle favorite:", error);
    }
  },

  moveItem: async (fromId: number, toId: number) => {
    try {
      await invoke("move_clipboard_item", { fromId, toId });
      // 刷新以获取更新后的顺序
      await get().refresh();
    } catch (error) {
      logError("Failed to move item:", error);
    }
  },

  deleteItem: async (id: number) => {
    try {
      await invoke("delete_clipboard_item", { id });
      set((state) => ({
        items: state.items.filter((item) => item.id !== id),
      }));
    } catch (error) {
      logError("Failed to delete item:", error);
    }
  },

  copyToClipboard: async (id: number) => {
    try {
      await invoke("copy_to_clipboard", { id });
    } catch (error) {
      logError("Failed to copy to clipboard:", error);
    }
  },

  pasteContent: async (id: number) => {
    await doPaste(get, id, "paste_content");
  },

  pasteAsPlainText: async (id: number) => {
    await doPaste(get, id, "paste_content_as_plain");
  },

  clearHistory: async (contentType = null) => {
    try {
      await invoke<number>("clear_history", {
        contentType,
      });
      await get().refresh();
    } catch (error) {
      logError("Failed to clear history:", error);
    }
  },

  refresh: async () => {
    await get().fetchItems();
  },

  resetView: async () => {
    // 仅重置搜索和类型筛选，保留标签选择
    set((state) => ({
      searchQuery: "",
      selectedCategory: null,
      batchMode: false,
      selectedIds: EMPTY_SET as Set<number>,
      lastSelectedIndex: -1,
      _resetToken: state._resetToken + 1,
    }));
    await get().fetchItems({ search: "" });
  },

  setupListener: async () => {
    const unlisten = await listen<number>("clipboard-updated", async () => {
      await get().refresh();
    });
    return unlisten;
  },

  // 批量选择
  batchMode: false,
  selectedIds: new Set<number>(),
  lastSelectedIndex: -1,

  setBatchMode: (enabled) => {
    set({ batchMode: enabled, selectedIds: EMPTY_SET as Set<number>, lastSelectedIndex: -1 });
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
    set({ selectedIds: EMPTY_SET as Set<number> });
  },

  batchDelete: async () => {
    const { selectedIds } = get();
    if (selectedIds.size === 0) return;
    try {
      await invoke("batch_delete_clipboard_items", { ids: Array.from(selectedIds) });
      set({ selectedIds: EMPTY_SET as Set<number>, batchMode: false, lastSelectedIndex: -1 });
      await get().refresh();
    } catch (error) {
      logError("Failed to batch delete:", error);
    }
  },
}));

