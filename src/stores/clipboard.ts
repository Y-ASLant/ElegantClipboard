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
  /** Whether all files exist (only for "files" content_type, computed at query time) */
  files_valid?: boolean;
}

interface ClipboardState {
  items: ClipboardItem[];
  pinnedItems: ClipboardItem[];
  totalCount: number;
  isLoading: boolean;
  searchQuery: string;
  selectedType: string | null;

  // Actions
  fetchItems: (options?: {
    search?: string;
    content_type?: string;
    limit?: number;
    offset?: number;
  }) => Promise<void>;
  fetchPinnedItems: () => Promise<void>;
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
  setupListener: () => Promise<() => void>;
}

export const useClipboardStore = create<ClipboardState>((set, get) => ({
  items: [],
  pinnedItems: [],
  totalCount: 0,
  isLoading: false,
  searchQuery: "",
  selectedType: null,

  fetchItems: async (options = {}) => {
    set({ isLoading: true });
    try {
      const state = get();
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        search: options.search ?? (state.searchQuery || null),
        contentType: options.content_type ?? state.selectedType,
        pinnedOnly: false,
        favoriteOnly: false,
        limit: options.limit ?? 100,
        offset: options.offset ?? 0,
      });
      set({ items, isLoading: false });
    } catch (error) {
      console.error("Failed to fetch items:", error);
      set({ isLoading: false });
    }
  },

  fetchPinnedItems: async () => {
    try {
      const items = await invoke<ClipboardItem[]>("get_clipboard_items", {
        pinnedOnly: true,
        limit: 50,
      });
      set({ pinnedItems: items });
    } catch (error) {
      console.error("Failed to fetch pinned items:", error);
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
      // Refresh pinned items
      get().fetchPinnedItems();
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
        pinnedItems: state.pinnedItems.filter((item) => item.id !== id),
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
      get().refresh();
    } catch (error) {
      console.error("Failed to clear history:", error);
    }
  },

  refresh: async () => {
    await Promise.all([
      get().fetchItems(),
      get().fetchPinnedItems(),
      get().fetchCount(),
    ]);
  },

  setupListener: async () => {
    const unlisten = await listen<number>("clipboard-updated", () => {
      get().refresh();
    });
    return unlisten;
  },
}));
