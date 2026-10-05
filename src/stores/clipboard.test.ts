import { createElement } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t } from "@/i18n";
import type { OperationResult } from "@/lib/operation-feedback";
import { useUISettings } from "@/stores/ui-settings";
import { useClipboardStore, type ClipboardItem } from "./clipboard";

function listItem(id: number): ClipboardItem {
  return {
    id,
    content_type: "text",
    text_content: null,
    html_content: null,
    rtf_content: null,
    image_path: null,
    file_paths: null,
    content_hash: `hash-${id}`,
    preview: `preview-${id}`,
    byte_size: 10,
    image_width: null,
    image_height: null,
    is_pinned: false,
    is_favorite: false,
    favorite_order: 0,
    sort_order: id,
    created_at: "2026-01-01T00:00:00",
    updated_at: "2026-01-01T00:00:00",
    access_count: 0,
    last_accessed_at: null,
    char_count: null,
    source_app_name: null,
    source_app_icon: null,
    group_id: null,
  };
}

// Reset store before each test
beforeEach(() => {
  vi.mocked(invoke).mockReset().mockResolvedValue(undefined);
  vi.mocked(listen).mockReset().mockResolvedValue(() => {});
  useUISettings.setState({ pasteMoveToTop: false });
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

afterEach(cleanup);

describe("clipboard store", () => {
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

  describe("clearHistory", () => {
    it("returns deleted count from backend and refreshes", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke)
        .mockResolvedValueOnce(5) // clear_history
        .mockResolvedValueOnce([]); // refresh -> get_clipboard_items

      const deleted = await useClipboardStore.getState().clearHistory(null);

      expect(deleted).toEqual({ status: "success", value: 5 });
      expect(invoke).toHaveBeenCalledWith("clear_history", {
        groupId: null,
        contentType: null,
      });
    });

    it("returns an explicit failed outcome", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      vi.mocked(invoke).mockRejectedValueOnce(new Error("db error"));

      const deleted = await useClipboardStore.getState().clearHistory(null);

      expect(deleted).toEqual({ status: "failed" });
    });
  });

  describe("operation outcomes", () => {
    it("keeps a failed deletion visible and exposes the failed outcome", async () => {
      const visible = listItem(7);
      useClipboardStore.setState({ items: [visible] });
      vi.mocked(invoke).mockRejectedValueOnce({ code: "permission_denied", detail: "database denied" });
      const result = await useClipboardStore.getState().deleteItem(7);
      expect(result).toEqual({ status: "failed" });
      expect(useClipboardStore.getState().items).toEqual([visible]);
    });

    it("keeps batch selection open after failure and closes it after success", async () => {
      useClipboardStore.setState({ items: [listItem(7)], batchMode: true, selectedIds: new Set([7]) });
      vi.mocked(invoke).mockRejectedValueOnce({ code: "permission_denied", detail: "delete denied" });
      expect(await useClipboardStore.getState().batchDelete()).toEqual({ status: "failed" });
      expect(useClipboardStore.getState().batchMode).toBe(true);
      expect([...useClipboardStore.getState().selectedIds]).toEqual([7]);
      vi.mocked(invoke).mockResolvedValueOnce(undefined).mockResolvedValueOnce([]);
      expect(await useClipboardStore.getState().batchDelete()).toEqual({ status: "success", value: undefined });
      expect(useClipboardStore.getState().batchMode).toBe(false);
      expect(useClipboardStore.getState().selectedIds.size).toBe(0);
      expect(useClipboardStore.getState().items).toEqual([]);
    });

    it("treats toggling favorite off as success and removes it from favorites view", async () => {
      useClipboardStore.setState({ items: [{ ...listItem(7), is_favorite: true }], selectedGroup: "__favorites__" });
      vi.mocked(invoke).mockResolvedValueOnce(false).mockResolvedValueOnce([]);
      expect(await useClipboardStore.getState().toggleFavorite(7)).toEqual({ status: "success", value: false });
      expect(useClipboardStore.getState().items).toEqual([]);
    });

    it("keeps a committed favorite change successful when only list refresh fails", async () => {
      render(createElement(Toaster));
      const visible = { ...listItem(7), is_favorite: true };
      useClipboardStore.setState({ items: [visible], selectedGroup: "__favorites__" });
      vi.mocked(invoke).mockResolvedValueOnce(false).mockRejectedValueOnce({ code: "internal", detail: "private refresh failure" });
      let outcome: OperationResult<boolean> | undefined;
      await act(async () => { outcome = await useClipboardStore.getState().toggleFavorite(7); });
      expect(outcome).toEqual({ status: "success", value: false });
      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(t("operationFeedback.refreshAfterSuccess"));
      expect(alert).not.toHaveTextContent(t("operationFeedback.operations.favorite"));
      expect(alert).not.toHaveTextContent("private refresh failure");
    });

    it("preserves paste success while reporting a separately failed reorder", async () => {
      render(createElement(Toaster));
      useUISettings.setState({ pasteMoveToTop: true });
      vi.mocked(invoke).mockResolvedValueOnce(undefined).mockRejectedValueOnce({ code: "internal", detail: "reorder failed" });
      let result: OperationResult | undefined;
      await act(async () => { result = await useClipboardStore.getState().pasteContent(7); });
      expect(result).toEqual({ status: "success", value: undefined });
      expect(await screen.findByRole("alert")).toHaveTextContent(t("operationFeedback.operations.reorder"));
      expect(screen.getAllByRole("alert")).toHaveLength(1);
    });

    it("does not refresh or reorder after native paste failure", async () => {
      const visible = listItem(7);
      useClipboardStore.setState({ items: [visible] });
      useUISettings.setState({ pasteMoveToTop: true });
      vi.mocked(invoke).mockRejectedValueOnce({ code: "paste_failed", detail: "target rejected" });
      expect(await useClipboardStore.getState().pasteContent(7)).toEqual({ status: "failed" });
      expect(useClipboardStore.getState().items).toEqual([visible]);
      expect(useClipboardStore.getState()._fetchId).toBe(0);
    });

    it("validates native shortcut failures, reports once, and stops on cleanup", async () => {
      render(createElement(Toaster));
      let onFailure!: EventCallback<unknown>;
      vi.mocked(listen).mockImplementation(async (event, handler) => {
        if (event === "clipboard-operation-failed") onFailure = handler as EventCallback<unknown>;
        return () => {};
      });
      const stop = await useClipboardStore.getState().setupListener();
      act(() => onFailure({ event: "clipboard-operation-failed", id: 1, payload: { operation: "paste", error: "raw unsafe" } }));
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      const failure = { operation: "paste", error: { code: "resource_missing", detail: "private path" } };
      act(() => onFailure({ event: "clipboard-operation-failed", id: 1, payload: failure }));
      await waitFor(() => expect(screen.getAllByRole("alert")).toHaveLength(1));
      expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.resource_missing"));
      expect(screen.getByRole("alert")).not.toHaveTextContent("private path");
      stop();
      act(() => onFailure({ event: "clipboard-operation-failed", id: 1, payload: failure }));
      expect(screen.getAllByRole("alert")).toHaveLength(1);
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

  describe("view request invalidation", () => {
    it("discards the previous view response during the search debounce gap", async () => {
      const visible = listItem(1);
      useClipboardStore.setState({ items: [visible] });
      let resolveOld!: (items: ClipboardItem[]) => void;
      vi.mocked(invoke).mockImplementationOnce(() => new Promise<ClipboardItem[]>((resolve) => {
        resolveOld = resolve;
      }));
      const pending = useClipboardStore.getState().fetchItems();

      useClipboardStore.getState().setSearchQuery("new search");
      resolveOld([listItem(2)]);
      await pending;

      expect(useClipboardStore.getState().items).toEqual([visible]);
      expect(useClipboardStore.getState().isLoading).toBe(false);
    });

    it("does not let a stale failure clear a newer view's loading state", async () => {
      let rejectOld!: (error: Error) => void;
      let resolveNew!: (items: ClipboardItem[]) => void;
      vi.mocked(invoke)
        .mockImplementationOnce(() => new Promise<ClipboardItem[]>((_, reject) => {
          rejectOld = reject;
        }))
        .mockImplementationOnce(() => new Promise<ClipboardItem[]>((resolve) => {
          resolveNew = resolve;
        }));
      const oldRequest = useClipboardStore.getState().fetchItems();
      useClipboardStore.getState().setSearchQuery("new search");
      const newRequest = useClipboardStore.getState().fetchItems();
      rejectOld(new Error("old view failed"));
      await oldRequest;
      expect(useClipboardStore.getState().isLoading).toBe(true);

      resolveNew([listItem(3)]);
      await newRequest;
      expect(useClipboardStore.getState().items.map((item) => item.id)).toEqual([3]);
      expect(useClipboardStore.getState().isLoading).toBe(false);
    });
  });

  describe("capture refresh", () => {
    let onCapture: EventCallback<number>;
    let cleanup: (() => void) | undefined;

    beforeEach(async () => {
      vi.useFakeTimers();
      vi.mocked(listen).mockImplementation(async (event, handler) => {
        if (event === "clipboard-updated") {
          onCapture = handler as EventCallback<number>;
        }
        return () => {};
      });
      cleanup = await useClipboardStore.getState().setupListener();
    });

    afterEach(() => {
      cleanup?.();
      vi.useRealTimers();
    });

    function capture(id: number) {
      onCapture({ event: "clipboard-updated", id: 1, payload: id });
    }

    it("removes evicted records and keeps only lightweight list data after capture", async () => {
      const retained = listItem(2);
      const captured = listItem(3);
      useClipboardStore.setState({ items: [listItem(1), retained] });
      vi.mocked(invoke).mockImplementation(async (command) => {
        if (command === "get_clipboard_item") {
          return { ...captured, text_content: "full captured body", html_content: "<p>full body</p>" };
        }
        return [captured, retained];
      });

      capture(3);
      await vi.advanceTimersByTimeAsync(50);

      expect(useClipboardStore.getState().items).toEqual([captured, retained]);
      expect(useClipboardStore.getState().items.every((item) =>
        item.text_content === null && item.html_content === null && item.rtf_content === null
      )).toBe(true);
    });

    it.each([
      { selectedGroup: "image,files,url", contentType: "image,files,url", favoriteOnly: false },
      { selectedGroup: "__favorites__", contentType: null, favoriteOnly: true },
    ])("preserves the custom group and $selectedGroup intersection", async ({
      selectedGroup, contentType, favoriteOnly,
    }) => {
      const visible = {
        ...listItem(1), group_id: 42, content_type: "image" as const, is_favorite: true,
      };
      const excludedCapture = { ...listItem(2), group_id: 42 };
      useClipboardStore.setState({ items: [visible], selectedGroupId: 42, selectedGroup });
      vi.mocked(invoke).mockImplementation(async (command) =>
        command === "get_clipboard_item" ? excludedCapture : [visible]
      );

      capture(2);
      await vi.advanceTimersByTimeAsync(50);

      expect(useClipboardStore.getState().items).toEqual([visible]);
      expect(invoke).toHaveBeenCalledWith("get_clipboard_items", expect.objectContaining({
        groupId: 42, contentType, favoriteOnly,
      }));
    });

    it("discards a capture response after the selected group changes", async () => {
      let resolveCapture!: (items: ClipboardItem[]) => void;
      vi.mocked(invoke).mockImplementationOnce(() => new Promise<ClipboardItem[]>((resolve) => {
        resolveCapture = resolve;
      }));
      capture(1);
      await vi.advanceTimersByTimeAsync(50);
      const currentView = { ...listItem(2), group_id: 42 };
      vi.mocked(invoke)
        .mockResolvedValueOnce(undefined)
        .mockResolvedValueOnce([currentView]);
      useClipboardStore.getState().setSelectedGroupId(42);
      await vi.advanceTimersByTimeAsync(0);
      resolveCapture([listItem(1)]);
      await vi.advanceTimersByTimeAsync(0);

      expect(useClipboardStore.getState().items).toEqual([currentView]);
    });

    it("refreshes during continuous captures instead of waiting for silence", async () => {
      const latest = listItem(99);
      useClipboardStore.setState({ items: [listItem(1)] });
      vi.mocked(invoke).mockResolvedValue([latest]);

      for (let index = 0; index < 15; index++) {
        capture(index + 2);
        await vi.advanceTimersByTimeAsync(20);
      }

      expect(useClipboardStore.getState().items).toEqual([latest]);
    });

    it("publishes slow capture refreshes and then catches up with events received in flight", async () => {
      let resolveFirst!: (items: ClipboardItem[]) => void;
      const firstSnapshot = listItem(2);
      const latestSnapshot = listItem(3);
      useClipboardStore.setState({ items: [listItem(1)] });
      vi.mocked(invoke)
        .mockImplementationOnce(() => new Promise<ClipboardItem[]>((resolve) => {
          resolveFirst = resolve;
        }))
        .mockResolvedValue([latestSnapshot]);
      capture(2);
      await vi.advanceTimersByTimeAsync(50);
      for (let index = 0; index < 30; index++) {
        capture(3);
        await vi.advanceTimersByTimeAsync(20);
      }
      resolveFirst([firstSnapshot]);
      await vi.advanceTimersByTimeAsync(0);
      expect(useClipboardStore.getState().items).toEqual([firstSnapshot]);

      await vi.advanceTimersByTimeAsync(50);
      expect(useClipboardStore.getState().items).toEqual([latestSnapshot]);
    });

    it("cancels a pending capture refresh when the listener is removed", async () => {
      const visible = listItem(1);
      useClipboardStore.setState({ items: [visible] });
      vi.mocked(invoke).mockResolvedValue([listItem(2)]);
      capture(2);
      cleanup?.();
      cleanup = undefined;
      await vi.advanceTimersByTimeAsync(500);

      expect(useClipboardStore.getState().items).toEqual([visible]);
    });
  });
});
