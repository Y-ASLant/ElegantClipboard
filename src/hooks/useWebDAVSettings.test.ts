import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { reportUserError } from "@/lib/operation-feedback";
import { notifyWebDAVAvailabilityChanged } from "@/lib/webdav-availability";
import { useWebDAVSettings } from "./useWebDAVSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/logger", () => ({ logError: vi.fn() }));
vi.mock("@/lib/operation-feedback", () => ({ reportUserError: vi.fn() }));
vi.mock("@/lib/webdav-availability", () => ({ notifyWebDAVAvailabilityChanged: vi.fn() }));

const mockInvoke = vi.mocked(invoke);
let backendSettings: Record<string, string>;

async function mountSettings() {
  const hook = renderHook(() => useWebDAVSettings());
  await act(async () => {});
  return hook;
}

async function advanceTimers(ms = 300) {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

function settingWrites() {
  return mockInvoke.mock.calls.filter(([command]) => command === "set_setting").map(([, args]) => args);
}

describe("useWebDAVSettings", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    backendSettings = {};
    mockInvoke.mockReset();
    mockInvoke.mockImplementation((command) => {
      if (command === "get_settings_batch") return Promise.resolve({ ...backendSettings });
      return Promise.resolve();
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it.each<Record<string, string>>([{}, { webdav_proxy_mode: "invalid", webdav_url: "https://webdav.example.com", webdav_enabled: "true" }])(
    "does not write loaded settings or normalized defaults back to the backend: %j",
    async (loaded) => {
      backendSettings = loaded;
      const { result } = await mountSettings();
      await advanceTimers();

      expect(result.current.remoteDir).toBe("/elegant-clipboard");
      expect(result.current.syncInterval).toBe("60");
      expect(result.current.proxyMode).toBe("system");
      expect(result.current.enabled).toBe(loaded.webdav_enabled === "true");
      expect(settingWrites()).toEqual([]);
      expect(notifyWebDAVAvailabilityChanged).not.toHaveBeenCalled();
    },
  );

  it("saves only the changed immediate setting and notifies after success", async () => {
    const { result } = await mountSettings();
    await act(async () => { result.current.setEnabled(true); });
    await advanceTimers();

    expect(settingWrites()).toEqual([{ key: "webdav_enabled", value: "true" }]);
    expect(notifyWebDAVAvailabilityChanged).toHaveBeenCalledTimes(1);
  });

  it("saves only the changed sync type", async () => {
    const { result } = await mountSettings();
    await act(async () => { result.current.setSyncTypes(new Set(["text", "files"])); });
    await advanceTimers();

    expect(settingWrites()).toEqual([{ key: "webdav_sync_image", value: "false" }]);
  });

  it("debounces edits and does not save a value reverted before the delay", async () => {
    const { result } = await mountSettings();
    act(() => { result.current.setUrl("https://first.example.com"); });
    await advanceTimers(200);
    act(() => { result.current.setUrl("https://latest.example.com"); });
    await advanceTimers(299);
    expect(settingWrites()).toEqual([]);
    await advanceTimers(1);
    expect(settingWrites()).toEqual([{ key: "webdav_url", value: "https://latest.example.com" }]);

    mockInvoke.mockClear();
    act(() => { result.current.setUrl("https://temporary.example.com"); });
    act(() => { result.current.setUrl("https://latest.example.com"); });
    await advanceTimers();
    expect(settingWrites()).toEqual([]);
  });

  it("reloads without writes and discards the previous pending debounce", async () => {
    const { result } = await mountSettings();
    act(() => { result.current.setUrl("https://pending.example.com"); });
    backendSettings = { webdav_url: "https://loaded.example.com", webdav_auto_sync: "true" };
    await act(async () => { await result.current.loadSettings(); });
    await advanceTimers();

    expect(result.current.url).toBe("https://loaded.example.com");
    expect(result.current.autoSync).toBe(true);
    expect(settingWrites()).toEqual([]);
  });

  it("keeps pending user edits when the settings page is unmounted", async () => {
    const { result, unmount } = await mountSettings();
    act(() => { result.current.setUrl("https://edited.example.com"); });
    unmount();
    await advanceTimers();

    expect(settingWrites()).toEqual([{ key: "webdav_url", value: "https://edited.example.com" }]);
  });

  it("retries failed fields on the next edit without rewriting successful fields", async () => {
    const { result } = await mountSettings();
    mockInvoke.mockRejectedValueOnce(new Error("save failed"));
    await act(async () => { result.current.setAutoSync(true); });
    expect(reportUserError).toHaveBeenCalledTimes(1);

    await act(async () => { result.current.setEnabled(true); });
    await act(async () => { result.current.setSyncInterval("120"); });
    await advanceTimers();

    expect(settingWrites()).toEqual([
      { key: "webdav_auto_sync", value: "true" },
      { key: "webdav_enabled", value: "true" },
      { key: "webdav_auto_sync", value: "true" },
      { key: "webdav_sync_interval", value: "120" },
    ]);
  });

  it("preserves an edit after reloading while an earlier save is in flight", async () => {
    const { result } = await mountSettings();
    let resolveSave!: () => void;
    mockInvoke.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveSave = resolve; }));
    await act(async () => { result.current.setSyncInterval("120"); });
    await act(async () => { await result.current.loadSettings(); });
    await act(async () => { result.current.setSyncInterval("180"); });
    await act(async () => { resolveSave(); });

    expect(settingWrites()).toEqual([
      { key: "webdav_sync_interval", value: "120" },
      { key: "webdav_sync_interval", value: "180" },
    ]);
  });

  it("persists a revert after reloading while the earlier value is still being saved", async () => {
    const { result } = await mountSettings();
    let resolveSave!: () => void;
    mockInvoke.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveSave = resolve; }));
    await act(async () => { result.current.setSyncInterval("120"); });
    await act(async () => { await result.current.loadSettings(); });
    await act(async () => { result.current.setSyncInterval("180"); });
    await act(async () => { result.current.setSyncInterval("60"); });
    await act(async () => { resolveSave(); });

    expect(settingWrites()).toEqual([
      { key: "webdav_sync_interval", value: "120" },
      { key: "webdav_sync_interval", value: "60" },
    ]);
  });

  it("does not duplicate an in-flight save and persists a later revert after it", async () => {
    const { result } = await mountSettings();
    let resolveSave!: () => void;
    mockInvoke.mockImplementationOnce(() => new Promise<void>((resolve) => { resolveSave = resolve; }));
    await act(async () => { result.current.setEnabled(true); });
    await act(async () => { result.current.setAutoSync(true); });
    expect(settingWrites().filter((args) => (args as { key: string }).key === "webdav_enabled")).toHaveLength(1);

    await act(async () => { result.current.setEnabled(false); });
    expect(settingWrites().filter((args) => (args as { key: string }).key === "webdav_enabled")).toHaveLength(1);
    await act(async () => { resolveSave(); });
    expect(settingWrites().filter((args) => (args as { key: string }).key === "webdav_enabled")).toEqual([
      { key: "webdav_enabled", value: "true" },
      { key: "webdav_enabled", value: "false" },
    ]);
  });
});
