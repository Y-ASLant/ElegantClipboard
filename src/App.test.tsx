import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { t } from "@/i18n";
import { useClipboardStore } from "@/stores/clipboard";
import { useGroupStore } from "@/stores/groups";
import { useUISettings } from "@/stores/ui-settings";
import App from "./App";

vi.mock("@/components/ClipboardList", () => ({ ClipboardList: () => null }));
vi.mock("@/hooks/useInputFocus", () => ({
  useInputFocus: () => ({ current: null }),
  focusWindowImmediately: vi.fn().mockResolvedValue(undefined),
  releaseWebViewFocus: vi.fn(),
}));
vi.mock("@/hooks/useWebDAVAvailable", () => ({ useWebDAVAvailable: () => false }));
vi.mock("@/stores/webdav-sync", () => ({ initWebDAVSyncListeners: vi.fn() }));

const listeners = new Map<string, EventCallback<unknown>>();

beforeEach(() => {
  vi.useFakeTimers();
  listeners.clear();
  vi.mocked(listen).mockReset().mockImplementation(async (event, callback) => {
    listeners.set(event, callback);
    return () => { listeners.delete(event); };
  });
  vi.mocked(invoke).mockReset().mockImplementation(async (command) => {
    if (command === "get_groups") return [{ id: 7, name: "Work", icon: null, sort_order: 0 }];
    if (command === "get_clipboard_items") return [];
    return null;
  });
  useClipboardStore.setState({
    items: [], isLoading: false, searchQuery: "", selectedGroup: null,
    selectedGroupId: null, batchMode: false, selectedIds: new Set(), _fetchId: 0,
  });
  useGroupStore.setState({ groups: [] });
  useUISettings.setState({
    showCategoryFilter: true, toolbarButtons: [], onboardingCompleted: true,
    searchAutoFocus: false, searchAutoClear: true, autoResetState: true,
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

async function renderApp() {
  render(<TooltipProvider><App /></TooltipProvider>);
  await act(async () => {});
  vi.mocked(invoke).mockClear();
}

function listRequests() {
  return vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_clipboard_items");
}

async function advance(ms: number) {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

describe("search request debounce", () => {
  it("queries the latest input once after the debounce delay", async () => {
    await renderApp();
    const input = screen.getByPlaceholderText(t("app.searchPlaceholder"));
    fireEvent.change(input, { target: { value: "first" } });
    await advance(100);
    fireEvent.change(input, { target: { value: "latest" } });
    await advance(299);
    expect(listRequests()).toHaveLength(0);
    await advance(1);
    expect(listRequests()).toHaveLength(1);
    expect(listRequests()[0][1]).toMatchObject({ search: "latest" });
  });

  it.each(["clear", "shown", "hidden", "category", "group", "same-category", "same-group"])(
    "cancels pending search when %s immediately queries the list",
    async (action) => {
      await renderApp();
      const input = screen.getByPlaceholderText(t("app.searchPlaceholder"));
      fireEvent.change(input, { target: { value: "pending" } });
      await advance(100);
      if (action === "clear") {
        fireEvent.click(input.parentElement!.querySelector("button")!);
      } else if (action === "shown" || action === "hidden") {
        const event = action === "shown" ? "window-shown" : "window-hidden";
        await act(async () => { listeners.get(event)!({ event, id: 1, payload: null }); });
      } else if (action === "category" || action === "same-category") {
        fireEvent.click(screen.getByRole("button", { name: t(action === "category" ? "groups.text" : "groups.all") }));
      } else {
        fireEvent.click(screen.getByRole("button", { name: t("groups.defaultGroup") }));
        if (action === "same-group") {
          fireEvent.click(screen.getAllByText(t("groups.defaultGroup"))[1]);
        } else {
          fireEvent.click(screen.getByText("Work"));
        }
      }
      await act(async () => {});
      expect(listRequests()).toHaveLength(1);
      await advance(400);
      expect(listRequests()).toHaveLength(1);
      expect(listRequests()[0][1]).toMatchObject({
        search: ["category", "group", "same-category", "same-group"].includes(action) ? "pending" : "",
        contentType: action === "category" ? "text,html,rtf" : null,
        groupId: action === "group" ? 7 : null,
      });
    },
  );
});
