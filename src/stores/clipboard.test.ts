import { describe, it, expect, vi, beforeEach } from "vitest";
import { useClipboardStore, type ClipboardItem } from "./clipboard";

// Reset store before each test
beforeEach(() => {
  useClipboardStore.setState({
    items: [],
    isLoading: false,
    searchQuery: "",
    selectedGroup: null,
    selectedGroupId: null,
    activeIndex: -1,
    batchMode: false,
    selectedIds: new Set(),
    lastSelectedIndex: -1,
    _fetchId: 0,
    _resetToken: 0,
  });
});

describe("clipboard store", () => {
  describe("initial state", () => {
    it("has correct defaults", () => {
      const state = useClipboardStore.getState();
      expect(state.items).toEqual([]);
      expect(state.isLoading).toBe(false);
      expect(state.searchQuery).toBe("");
      expect(state.selectedGroup).toBeNull();
      expect(state.selectedGroupId).toBeNull();
      expect(state.activeIndex).toBe(-1);
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
    });
  });

  describe("setSearchQuery", () => {
    it("updates search query", () => {
      useClipboardStore.getState().setSearchQuery("test query");
      expect(useClipboardStore.getState().searchQuery).toBe("test query");
    });

    it("handles empty string", () => {
      useClipboardStore.getState().setSearchQuery("something");
      useClipboardStore.getState().setSearchQuery("");
      expect(useClipboardStore.getState().searchQuery).toBe("");
    });
  });

  describe("setSelectedGroup", () => {
    it("updates selected group and resets batch state", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2, 3]),
        lastSelectedIndex: 5,
      });

      useClipboardStore.getState().setSelectedGroup("text");

      const state = useClipboardStore.getState();
      expect(state.selectedGroup).toBe("text");
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state.lastSelectedIndex).toBe(-1);
    });

    it("sets group to null", () => {
      useClipboardStore.getState().setSelectedGroup("image");
      useClipboardStore.getState().setSelectedGroup(null);
      expect(useClipboardStore.getState().selectedGroup).toBeNull();
    });
  });

  describe("setSelectedGroupId", () => {
    it("updates group id and persists", () => {
      useClipboardStore.getState().setSelectedGroupId(42);
      expect(useClipboardStore.getState().selectedGroupId).toBe(42);
    });

    it("resets batch state", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1]),
      });
      useClipboardStore.getState().setSelectedGroupId(1);
      expect(useClipboardStore.getState().batchMode).toBe(false);
      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
    });
  });

  describe("setActiveIndex", () => {
    it("updates active index", () => {
      useClipboardStore.getState().setActiveIndex(5);
      expect(useClipboardStore.getState().activeIndex).toBe(5);
    });

    it("resets to -1", () => {
      useClipboardStore.getState().setActiveIndex(3);
      useClipboardStore.getState().setActiveIndex(-1);
      expect(useClipboardStore.getState().activeIndex).toBe(-1);
    });
  });

  describe("deleteItem", () => {
    it("removes item from list after invoke", async () => {
      const mockItems: ClipboardItem[] = [
        { id: 1, content_type: "text", text_content: "a", preview: "a" } as ClipboardItem,
        { id: 2, content_type: "text", text_content: "b", preview: "b" } as ClipboardItem,
        { id: 3, content_type: "text", text_content: "c", preview: "c" } as ClipboardItem,
      ];
      useClipboardStore.setState({ items: mockItems });

      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockResolvedValueOnce(undefined);

      await useClipboardStore.getState().deleteItem(2);

      const items = useClipboardStore.getState().items;
      expect(items).toHaveLength(2);
      expect(items.find((i) => i.id === 2)).toBeUndefined();
    });
  });

  describe("batch selection", () => {
    it("toggleSelect adds item to selection", () => {
      const mockItems: ClipboardItem[] = [
        { id: 1 } as ClipboardItem,
        { id: 2 } as ClipboardItem,
        { id: 3 } as ClipboardItem,
      ];
      useClipboardStore.setState({ items: mockItems, batchMode: true });

      useClipboardStore.getState().toggleSelect(2, 1, false);

      const state = useClipboardStore.getState();
      expect(state.selectedIds.has(2)).toBe(true);
      expect(state.lastSelectedIndex).toBe(1);
    });

    it("toggleSelect removes item if already selected", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2]),
      });

      useClipboardStore.getState().toggleSelect(2, 1, false);

      expect(useClipboardStore.getState().selectedIds.has(2)).toBe(false);
    });

    it("shift+click selects range", () => {
      const mockItems: ClipboardItem[] = Array.from({ length: 10 }, (_, i) => ({
        id: i + 1,
      })) as ClipboardItem[];
      useClipboardStore.setState({
        items: mockItems,
        batchMode: true,
        lastSelectedIndex: 2,
      });

      useClipboardStore.getState().toggleSelect(6, 5, true);

      const selected = useClipboardStore.getState().selectedIds;
      // Should select items at indices 2-5 (ids 3-6)
      expect(selected.has(3)).toBe(true);
      expect(selected.has(4)).toBe(true);
      expect(selected.has(5)).toBe(true);
      expect(selected.has(6)).toBe(true);
      expect(selected.has(1)).toBe(false);
      expect(selected.has(7)).toBe(false);
    });

    it("selectAll selects all items", () => {
      const mockItems: ClipboardItem[] = Array.from({ length: 5 }, (_, i) => ({
        id: i + 1,
      })) as ClipboardItem[];
      useClipboardStore.setState({ items: mockItems, batchMode: true });

      useClipboardStore.getState().selectAll();

      expect(useClipboardStore.getState().selectedIds.size).toBe(5);
    });

    it("deselectAll clears selection", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2, 3]),
      });

      useClipboardStore.getState().deselectAll();

      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
    });

    it("setBatchMode resets selection", () => {
      useClipboardStore.setState({
        batchMode: true,
        selectedIds: new Set([1, 2]),
        lastSelectedIndex: 3,
      });

      useClipboardStore.getState().setBatchMode(false);

      const state = useClipboardStore.getState();
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state.lastSelectedIndex).toBe(-1);
    });
  });

  describe("resetView", () => {
    it("resets search and group, increments resetToken", () => {
      useClipboardStore.setState({
        searchQuery: "test",
        selectedGroup: "text",
        batchMode: true,
        selectedIds: new Set([1]),
        lastSelectedIndex: 5,
        _resetToken: 3,
      });

      useClipboardStore.getState().resetView();

      const state = useClipboardStore.getState();
      expect(state.searchQuery).toBe("");
      expect(state.selectedGroup).toBeNull();
      expect(state.batchMode).toBe(false);
      expect(state.selectedIds.size).toBe(0);
      expect(state._resetToken).toBe(4);
    });
  });

  describe("fetchItems deduplication", () => {
    it("discards stale responses using _fetchId", async () => {
      const { invoke } = await import("@tauri-apps/api/core");

      // First call returns slow
      let resolveFirst: (value: ClipboardItem[]) => void;
      const firstPromise = new Promise<ClipboardItem[]>((r) => {
        resolveFirst = r;
      });
      vi.mocked(invoke).mockImplementationOnce(() => firstPromise);

      // Second call returns fast
      const fastItems = [{ id: 99 } as ClipboardItem];
      vi.mocked(invoke).mockResolvedValueOnce(fastItems);

      // Start both fetches
      const fetch1 = useClipboardStore.getState().fetchItems();
      const fetch2 = useClipboardStore.getState().fetchItems();

      // Resolve fast one first
      resolveFirst!([]);
      await Promise.all([fetch1, fetch2]);

      // Should have the fast result (fetch2), not the stale one
      expect(useClipboardStore.getState().items).toEqual(fastItems);
    });
  });
});
