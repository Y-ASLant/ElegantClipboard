import { describe, it, expect, vi, beforeEach } from "vitest";
import { useClipboardStore } from "./clipboard";
import { useGroupStore } from "./groups";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string, args?: Record<string, unknown>) => {
    if (command === "get_groups") {
      return Promise.resolve([
        { id: 1, name: "Work", color: "#ff0000", sort_order: 0, created_at: new Date().toISOString(), item_count: 5 },
        { id: 2, name: "Personal", color: null, sort_order: 1, created_at: new Date().toISOString(), item_count: 3 },
      ]);
    }
    if (command === "create_group") {
      return Promise.resolve({
        id: 3,
        name: args?.name ?? "New",
        color: args?.color ?? null,
        sort_order: 2,
        created_at: new Date().toISOString(),
        item_count: 0,
      });
    }
    return Promise.resolve();
  }),
}));

vi.mock("@/lib/logger", () => ({
  logError: vi.fn(),
}));

const mockFetchItems = vi.fn();
vi.mock("@/stores/clipboard", () => ({
  useClipboardStore: {
    getState: vi.fn(() => ({
      fetchItems: mockFetchItems,
    })),
  },
}));

beforeEach(() => {
  useGroupStore.setState({ groups: [], isLoading: false });
  mockFetchItems.mockClear();
});

describe("group store", () => {
  describe("initial state", () => {
    it("has correct defaults", () => {
      const state = useGroupStore.getState();
      expect(state.groups).toEqual([]);
      expect(state.isLoading).toBe(false);
    });
  });

  describe("fetchGroups", () => {
    it("fetches groups and updates state", async () => {
      await useGroupStore.getState().fetchGroups();
      const state = useGroupStore.getState();
      expect(state.groups).toHaveLength(2);
      expect(state.groups[0].name).toBe("Work");
      expect(state.isLoading).toBe(false);
    });
  });

  describe("createGroup", () => {
    it("creates a group and adds to state", async () => {
      const group = await useGroupStore.getState().createGroup("New Group");
      expect(group).not.toBeNull();
      expect(group?.name).toBe("New Group");
      expect(useGroupStore.getState().groups).toHaveLength(1);
    });

    it("returns null on failure", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValueOnce(new Error("fail"));
      const group = await useGroupStore.getState().createGroup("Fail Group");
      expect(group).toBeNull();
    });
  });

  describe("renameGroup", () => {
    it("renames a group", async () => {
      await useGroupStore.getState().fetchGroups();
      await useGroupStore.getState().renameGroup(1, "Updated");
      const state = useGroupStore.getState();
      expect(state.groups.find((g) => g.id === 1)?.name).toBe("Updated");
    });
  });

  describe("updateGroupColor", () => {
    it("updates group color", async () => {
      await useGroupStore.getState().fetchGroups();
      await useGroupStore.getState().updateGroupColor(1, "#00ff00");
      const state = useGroupStore.getState();
      expect(state.groups.find((g) => g.id === 1)?.color).toBe("#00ff00");
    });
  });

  describe("deleteGroup", () => {
    it("deletes a group", async () => {
      await useGroupStore.getState().fetchGroups();
      expect(useGroupStore.getState().groups).toHaveLength(2);
      await useGroupStore.getState().deleteGroup(1);
      expect(useGroupStore.getState().groups).toHaveLength(1);
    });
  });

  describe("moveItemToGroup", () => {
    it("calls invoke and refreshes", async () => {
      await useGroupStore.getState().moveItemToGroup(1, 2);
      expect(mockFetchItems).toHaveBeenCalled();
    });
  });
});
