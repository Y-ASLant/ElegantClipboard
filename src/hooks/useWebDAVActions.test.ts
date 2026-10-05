import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) => {
    if (command === "webdav_test_connection") {
      return Promise.resolve("Connection successful");
    }
    if (command === "webdav_upload") {
      return Promise.resolve({
        message: "Upload complete",
        pending_media_workers: 0,
      });
    }
    if (command === "webdav_download") {
      return Promise.resolve({
        message: "Download complete",
        pending_media_workers: 0,
      });
    }
    return Promise.resolve();
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/stores/clipboard", () => ({
  useClipboardStore: {
    getState: () => ({ refresh: vi.fn(() => Promise.resolve()) }),
  },
}));

vi.mock("@/stores/ui-settings", () => ({
  loadUISettingsFromBackend: vi.fn(() => Promise.resolve()),
}));

describe("useWebDAVActions", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    const { resetWebDAVSyncStoreForTests } = await import("@/stores/webdav-sync");
    resetWebDAVSyncStoreForTests();
  });

  it("exports useWebDAVActions hook", async () => {
    const mod = await import("./useWebDAVActions");
    expect(typeof mod.useWebDAVActions).toBe("function");
  });

  it("returns correct initial state", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { result } = renderHook(() => useWebDAVActions());
    
    expect(result.current.testing).toBe(false);
    expect(result.current.syncing).toBe(false);
    expect(result.current.statusMsg).toBe("");
    expect(result.current.statusType).toBe("info");
  });

  it("handleTestConnection calls invoke", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { invoke } = await import("@tauri-apps/api/core");
    const { result } = renderHook(() => useWebDAVActions());
    
    await act(async () => {
      await result.current.handleTestConnection();
    });
    
    expect(invoke).toHaveBeenCalledWith("webdav_test_connection");
    expect(result.current.statusMsg).toBe("Connection successful");
    expect(result.current.statusType).toBe("success");
  });

  it("handleUpload calls invoke", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { invoke } = await import("@tauri-apps/api/core");
    const { result } = renderHook(() => useWebDAVActions());
    
    await act(async () => {
      await result.current.handleUpload();
    });
    
    expect(invoke).toHaveBeenCalledWith("webdav_upload");
    expect(result.current.statusMsg).toBe("Upload complete");
  });

  it("reports a completed download", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { invoke } = await import("@tauri-apps/api/core");
    const { result } = renderHook(() => useWebDAVActions());
    
    await act(async () => {
      await result.current.handleDownload();
    });
    
    expect(invoke).toHaveBeenCalledWith("webdav_download");
    expect(result.current.statusMsg).toBe("Download complete");
  });
});
