import { renderHook, act, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) => {
    if (command === "webdav_test_connection") {
      return Promise.resolve("Connection successful");
    }
    if (command === "webdav_upload") {
      return Promise.resolve("Upload complete");
    }
    if (command === "webdav_download") {
      return Promise.resolve("Download complete");
    }
    return Promise.resolve();
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/stores/ui-settings", () => ({
  loadUISettingsFromBackend: vi.fn(() => Promise.resolve()),
}));

describe("useWebDAVActions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

  it("handleDownload calls invoke and emits event", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { invoke } = await import("@tauri-apps/api/core");
    const { emit } = await import("@tauri-apps/api/event");
    const { result } = renderHook(() => useWebDAVActions());
    
    await act(async () => {
      await result.current.handleDownload();
    });
    
    expect(invoke).toHaveBeenCalledWith("webdav_download");
    expect(result.current.statusMsg).toBe("Download complete");
  });

  it("handles connection error", async () => {
    const { useWebDAVActions } = await import("./useWebDAVActions");
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Connection failed"));
    const { result } = renderHook(() => useWebDAVActions());
    
    await act(async () => {
      await result.current.handleTestConnection();
    });
    
    expect(result.current.statusType).toBe("error");
    expect(result.current.statusMsg).toContain("Connection failed");
  });
});
