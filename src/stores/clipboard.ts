import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

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
  totalCount: number;
  isLoading: boolean;
  searchQuery: string;
  selectedType: string | null;
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
  fetchCount: () => Promise<void>;
  setSearchQuery: (query: string) => void;
  setSelectedType: (type: string | null) => void;
  togglePin: (id: number) => Promise<void>;
  toggleFavorite: (id: number) => Promise<void>;
  moveItem: (fromId: number, toId: number) => Promise<void>;
  deleteItem: (id: number) => Promise<void>;
  copyToClipboard: (id: number) => Promise<void>;
  pasteContent: (id: number) => Promise<void>;
  clearHistory: () => Promise<void>;
  refresh: () => Promise<void>;
  /** Reset view state: clear search, clear type filter, scroll to top, refresh */
  resetView: () => Promise<void>;
  setupListener: () => Promise<() => void>;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  totalCount: 0,
  isLoading: false,
  searchQuery: "",
  selectedType: null,
  _fetchId: 0,
  _resetToken: 0,

  fetchItems: async (options = {}) => {
    const state = get();
    const fetchId = state._fetchId + 1;
    set({ isLoading: true, _fetchId: fetchId });
    try {
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: options.search ?? (state.searchQuery || null),
        contentType: options.content_type ?? state.selectedType,
        pinnedOnly: false,
        favoriteOnly: false,
        limit: options.limit ?? null,
        offset: options.offset ?? 0,
      });
      // Only apply result if no newer fetch has started
      if (get()._fetchId === fetchId) {
        set({ items, isLoading: false });
      }
    } catch (error) {
      if (get()._fetchId === fetchId) {
        console.error("Failed to fetch items:", error);
        set({ isLoading: false });
      }
    }
  },

  fetchCount: async () => {
    try {
      const count = await invoke<number>("get_clipboard_count", {});
      set({ totalCount: count });
    } catch (error) {
      console.error("Failed to fetch count:", error);
    }
  },

  setSearchQuery: (query: string) => {
    set({ searchQuery: query });
    // Note: Debouncing is handled in App.tsx with useMemo + debounce
    // This just updates the query state
  },

  setSelectedType: (type: string | null) => {
    set({ selectedType: type });
    get().fetchItems();
  },

  togglePin: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_pin", { id });
      // Update local state
      set((state) => ({
        items: state.items.map((item) =>
          item.id === id ? { ...item, is_pinned: newState } : item
        ),
      }));
    } catch (error) {
      console.error("Failed to toggle pin:", error);
    }
  },

  toggleFavorite: async (id: number) => {
    try {
      const newState = await invoke<boolean>("toggle_favorite", { id });
      set((state) => ({
        items: state.items.map((item) =>
          item.id === id ? { ...item, is_favorite: newState } : item
        ),
      }));
    } catch (error) {
      console.error("Failed to toggle favorite:", error);
    }
  },

  moveItem: async (fromId: number, toId: number) => {
    try {
      await invoke("move_clipboard_item", { fromId, toId });
      // Refresh to get updated order
      await get().refresh();
    } catch (error) {
      console.error("Failed to move item:", error);
    }
  },

  deleteItem: async (id: number) => {
    try {
      await invoke("delete_clipboard_item", { id });
      set((state) => ({
        items: state.items.filter((item) => item.id !== id),
        totalCount: state.totalCount - 1,
      }));
    } catch (error) {
      console.error("Failed to delete item:", error);
    }
  },

  copyToClipboard: async (id: number) => {
    try {
      await invoke("copy_to_clipboard", { id });
    } catch (error) {
      console.error("Failed to copy to clipboard:", error);
    }
  },

  pasteContent: async (id: number) => {
    try {
      await invoke("paste_content", { id });
    } catch (error) {
      console.error("Failed to paste content:", error);
    }
  },

  clearHistory: async () => {
    try {
      await invoke<number>("clear_history");
      await get().refresh();
    } catch (error) {
      console.error("Failed to clear history:", error);
    }
  },

  refresh: async () => {
    await get().fetchItems();
  },

  resetView: async () => {
    set((state) => ({
      searchQuery: "",
      selectedType: null,
      _resetToken: state._resetToken + 1,
    }));
    await get().fetchItems({ search: "" });
  },

  setupListener: async () => {
    const unlisten = await listen<number>("clipboard-updated", () => {
      get().refresh();
    });
    return unlisten;
  },
}));
