import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { cancelPendingFocusRestore } from "@/hooks/useInputFocus";
import { logError } from "@/lib/logger";
import { playCopySound, playPasteSound } from "@/lib/sounds";
import { useUISettings } from "@/stores/ui-settings";

export interface ClipboardItem {
  id: number;
  content_type: "text" | "image" | "html" | "rtf" | "files";
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
  /** Whether all files exist (only for "files" content_type, computed at query time) */
  files_valid?: boolean;
}

interface ClipboardState {
  items: ClipboardItem[];
  isLoading: boolean;
  searchQuery: string;
  selectedGroup: string | null;
  /** Currently keyboard-highlighted item index (-1 = none) */
  activeIndex: number;
  /** Monotonic counter to discard stale fetch results */
  _fetchId: number;
  /** Incremented when the view should reset (scroll to top, etc.) */
  _resetToken: number;

  // Actions
  fetchItems: (options?: {
    search?: string;
    content_type?: string;
    limit?: number;
    offset?: number;
  }) => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSelectedGroup: (group: string | null) => void;
  setActiveIndex: (index: number) => void;
  togglePin: (id: number) => Promise<void>;
  toggleFavorite: (id: number) => Promise<void>;
  moveItem: (fromId: number, toId: number) => Promise<void>;
  deleteItem: (id: number) => Promise<void>;
  copyToClipboard: (id: number) => Promise<void>;
  pasteContent: (id: number) => Promise<void>;
  pasteAsPlainText: (id: number) => Promise<void>;
  clearHistory: () => Promise<void>;
  refresh: () => Promise<void>;
  /** Reset view state: clear search, clear type filter, scroll to top, refresh */
  resetView: () => Promise<void>;
  setupListener: () => Promise<() => void>;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  isLoading: false,
  searchQuery: "",
  selectedGroup: null,
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
        limit: options.limit ?? null,
        offset: options.offset ?? 0,
      });
      if (get()._fetchId === fetchId) {
        set({ items, isLoading: false, activeIndex: -1 });
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
    // Note: Debouncing is handled in App.tsx with useMemo + debounce
    // This just updates the query state
  },

  setSelectedGroup: (group: string | null) => {
    set({ selectedGroup: group });
    get().fetchItems();
  },

  setActiveIndex: (index: number) => {
    set({ activeIndex: index });
  },

  togglePin: async (id: number) => {
    try {
      await invoke<boolean>("toggle_pin", { id });
      // Refresh to get correct sort order (pinned items first)
      await get().refresh();
    } catch (error) {
      logError("Failed to toggle pin:", error);
    }
  },

  toggleFavorite: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_favorite", { id });
      // 在收藏视图中取消收藏时，需要刷新列表以移除该条目
      if (!newState && get().selectedGroup === "__favorites__") {
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
      // Refresh to get updated order
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
    try {
      cancelPendingFocusRestore();
      playPasteSound();
      const closeWindow = useUISettings.getState().pasteCloseWindow;
      await invoke("paste_content", { id, closeWindow });
    } catch (error) {
      logError("Failed to paste content:", error);
    }
  },

  pasteAsPlainText: async (id: number) => {
    try {
      cancelPendingFocusRestore();
      playPasteSound();
      const closeWindow = useUISettings.getState().pasteCloseWindow;
      await invoke("paste_content_as_plain", { id, closeWindow });
    } catch (error) {
      logError("Failed to paste as plain text:", error);
    }
  },

  clearHistory: async () => {
    try {
      await invoke<number>("clear_history");
      await get().refresh();
    } catch (error) {
      logError("Failed to clear history:", error);
    }
  },

  refresh: async () => {
    await get().fetchItems();
  },

  resetView: async () => {
    set((state) => ({
      searchQuery: "",
      selectedGroup: null,
      _resetToken: state._resetToken + 1,
    }));
    await get().fetchItems({ search: "" });
  },

  setupListener: async () => {
    const unlisten = await listen<number>("clipboard-updated", () => {
      playCopySound();
      get().refresh();
    });
    return unlisten;
  },
}));

