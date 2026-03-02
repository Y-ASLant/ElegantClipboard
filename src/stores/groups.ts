import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { logError } from "@/lib/logger";
import { useClipboardStore } from "@/stores/clipboard";

export interface Group {
  id: number;
  name: string;
  color: string | null;
  sort_order: number;
  created_at: string;
  item_count: number;
}

interface GroupState {
  groups: Group[];
  isLoading: boolean;

  fetchGroups: () => Promise<void>;
  createGroup: (name: string, color?: string) => Promise<Group | null>;
  renameGroup: (id: number, name: string) => Promise<void>;
  updateGroupColor: (id: number, color: string | null) => Promise<void>;
  deleteGroup: (id: number) => Promise<void>;
  moveItemToGroup: (itemId: number, groupId: number | null) => Promise<void>;
}

export const useGroupStore = create<GroupState>((set, get) => ({
  groups: [],
  isLoading: false,

  fetchGroups: async () => {
    set({ isLoading: true });
    try {
      const groups = await invoke<Group[]>("get_groups");
      set({ groups, isLoading: false });
    } catch (error) {
      logError("Failed to fetch groups:", error);
      set({ isLoading: false });
    }
  },

  createGroup: async (name, color) => {
    try {
      const group = await invoke<Group>("create_group", {
        name,
        color: color ?? null,
      });
      set((state) => ({ groups: [...state.groups, group] }));
      return group;
    } catch (error) {
      logError("Failed to create group:", error);
      return null;
    }
  },

  renameGroup: async (id, name) => {
    try {
      await invoke("rename_group", { id, name });
      set((state) => ({
        groups: state.groups.map((g) => (g.id === id ? { ...g, name } : g)),
      }));
    } catch (error) {
      logError("Failed to rename group:", error);
    }
  },

  updateGroupColor: async (id, color) => {
    try {
      await invoke("update_group_color", { id, color });
      set((state) => ({
        groups: state.groups.map((g) => (g.id === id ? { ...g, color } : g)),
      }));
    } catch (error) {
      logError("Failed to update group color:", error);
    }
  },

  deleteGroup: async (id) => {
    try {
      await invoke("delete_group", { id });
      set((state) => ({
        groups: state.groups.filter((g) => g.id !== id),
      }));
    } catch (error) {
      logError("Failed to delete group:", error);
    }
  },

  moveItemToGroup: async (itemId, groupId) => {
    try {
      await invoke("move_item_to_group", { itemId, groupId });
      // 刷新分组列表以更新 item_count，并刷新剪贴板列表移除已移走的条目
      get().fetchGroups();
      useClipboardStore.getState().fetchItems();
    } catch (error) {
      logError("Failed to move item to group:", error);
    }
  },
}));
