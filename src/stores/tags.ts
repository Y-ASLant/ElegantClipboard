import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { logError } from "@/lib/logger";
import { removeVisibleClipboardItem, upsertVisibleClipboardItem, useClipboardStore, type ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";

export interface Tag {
  id: number;
  name: string;
  sort_order: number;
  created_at: string;
  item_count: number;
}

interface TagState {
  tags: Tag[];
  isLoading: boolean;

  fetchTags: () => Promise<void>;
  createTag: (name: string) => Promise<Tag | null>;
  renameTag: (id: number, name: string) => Promise<void>;
  deleteTag: (id: number) => Promise<void>;
  addTagToItem: (itemId: number, tagId: number) => Promise<void>;
  removeTagFromItem: (itemId: number, tagId: number) => Promise<void>;
  getItemTags: (itemId: number) => Promise<Tag[]>;
  reorderTags: (tagIds: number[]) => Promise<void>;
  reorderTagItems: (tagId: number, itemIds: number[]) => Promise<void>;
}

export const useTagStore = create<TagState>((set, get) => ({
  tags: [],
  isLoading: false,

  fetchTags: async () => {
    set({ isLoading: true });
    try {
      const tags = await invoke<Tag[]>("get_tags");
      set({ tags, isLoading: false });
    } catch (error) {
      logError("Failed to fetch tags:", error);
      set({ isLoading: false });
    }
  },

  createTag: async (name) => {
    try {
      const tag = await invoke<Tag>("create_tag", { name });
      set((state) => ({ tags: [...state.tags, tag] }));
      return tag;
    } catch (error) {
      logError("Failed to create tag:", error);
      return null;
    }
  },

  renameTag: async (id, name) => {
    try {
      await invoke("rename_tag", { id, name });
      set((state) => ({
        tags: state.tags.map((t) => (t.id === id ? { ...t, name } : t)),
      }));
    } catch (error) {
      logError("Failed to rename tag:", error);
    }
  },

  deleteTag: async (id) => {
    try {
      await invoke("delete_tag", { id });
      set((state) => ({
        tags: state.tags.filter((t) => t.id !== id),
      }));
      // 如果当前正在按此标签过滤，则清除过滤
      const clipboardState = useClipboardStore.getState();
      if (clipboardState.selectedTagId === id) {
        clipboardState.setSelectedTagId(null);
      }
    } catch (error) {
      logError("Failed to delete tag:", error);
    }
  },

  addTagToItem: async (itemId, tagId) => {
    try {
      await invoke("add_tag_to_item", { itemId, tagId });
      await get().fetchTags();

      const { selectedCategory, selectedTagId, searchQuery } = useClipboardStore.getState();
      const isMainView = selectedCategory !== "__favorites__" && !selectedTagId;
      if (isMainView && useUISettings.getState().hideTaggedFromMain) {
        if (searchQuery) {
          await useClipboardStore.getState().fetchItems();
        } else {
          removeVisibleClipboardItem(itemId);
        }
      }
    } catch (error) {
      logError("Failed to add tag to item:", error);
    }
  },

  removeTagFromItem: async (itemId, tagId) => {
    try {
      await invoke("remove_tag_from_item", { itemId, tagId });
      await get().fetchTags();

      const { selectedCategory, selectedTagId, searchQuery } = useClipboardStore.getState();
      const isCurrentTagView = selectedTagId === tagId;
      const isMainView = selectedCategory !== "__favorites__" && !selectedTagId;
      const shouldShowInHiddenTaggedMain = isMainView && useUISettings.getState().hideTaggedFromMain;

      if (isCurrentTagView) {
        removeVisibleClipboardItem(itemId);
      } else if (shouldShowInHiddenTaggedMain) {
        if (searchQuery) {
          await useClipboardStore.getState().fetchItems();
        } else {
          const stillTagged = await invoke<boolean>("item_has_tags", { itemId });
          if (!stillTagged) {
            const item = await invoke<ClipboardItem | null>("get_clipboard_item", { id: itemId });
            if (item) upsertVisibleClipboardItem(item);
          }
        }
      }
    } catch (error) {
      logError("Failed to remove tag from item:", error);
    }
  },

  getItemTags: async (itemId) => {
    try {
      return await invoke<Tag[]>("get_item_tags", { itemId });
    } catch (error) {
      logError("Failed to get item tags:", error);
      return [];
    }
  },

  reorderTagItems: async (tagId, itemIds) => {
    try {
      await invoke("reorder_tag_items", { tagId, itemIds });
    } catch (error) {
      logError("Failed to reorder tag items:", error);
    }
  },

  reorderTags: async (tagIds) => {
    try {
      // Optimistic update
      set((state) => {
        const tagMap = new Map(state.tags.map((t) => [t.id, t]));
        const reordered = tagIds
          .map((id) => tagMap.get(id))
          .filter((t): t is Tag => t !== undefined);
        return { tags: reordered };
      });
      await invoke("reorder_tags", { tagIds });
    } catch (error) {
      logError("Failed to reorder tags:", error);
      // Rollback: re-fetch from backend
      get().fetchTags();
    }
  },
}));
