import { renderHook, act, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string, args?: Record<string, unknown>) => {
    if (command === "get_app_setting") {
      const key = args?.key as string;
      const defaults: Record<string, string> = {
        webdav_enabled: "false",
        webdav_auto_sync: "false",
        webdav_sync_interval: "60",
        webdav_url: "",
        webdav_username: "",
        webdav_password: "",
        webdav_remote_dir: "/elegant-clipboard",
        webdav_proxy_mode: "system",
        webdav_proxy_url: "",
        webdav_accept_invalid_certs: "false",
        webdav_sync_types: "text,image,files",
        webdav_max_image_size_kb: "5120",
        webdav_max_file_size_kb: "5120",
        webdav_max_video_size_kb: "5120",
        webdav_last_sync_time: "",
      };
      return Promise.resolve(defaults[key] ?? "");
    }
    if (command === "set_app_setting") {
      return Promise.resolve();
    }
    return Promise.resolve();
  }),
}));

vi.mock("@/lib/logger", () => ({
  logError: vi.fn(),
}));

describe("useWebDAVSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("exports useWebDAVSettings hook", async () => {
    const mod = await import("./useWebDAVSettings");
    expect(typeof mod.useWebDAVSettings).toBe("function");
  });

  it("returns correct initial defaults after load", async () => {
    const { useWebDAVSettings } = await import("./useWebDAVSettings");
    const { result } = renderHook(() => useWebDAVSettings());
    
    await waitFor(() => {
      expect(result.current.remoteDir).toBe("/elegant-clipboard");
    });
    
    expect(result.current.enabled).toBe(false);
    expect(result.current.autoSync).toBe(false);
    expect(result.current.syncInterval).toBe("60");
    expect(result.current.proxyMode).toBe("system");
  });

  it("updates enabled setting", async () => {
    const { useWebDAVSettings } = await import("./useWebDAVSettings");
    const { result } = renderHook(() => useWebDAVSettings());
    
    await waitFor(() => {
      expect(result.current.remoteDir).toBe("/elegant-clipboard");
    });
    
    act(() => {
      result.current.setEnabled(true);
    });
    
    expect(result.current.enabled).toBe(true);
  });

  it("updates url setting", async () => {
    const { useWebDAVSettings } = await import("./useWebDAVSettings");
    const { result } = renderHook(() => useWebDAVSettings());
    
    await waitFor(() => {
      expect(result.current.remoteDir).toBe("/elegant-clipboard");
    });
    
    act(() => {
      result.current.setUrl("https://webdav.example.com");
    });
    
    expect(result.current.url).toBe("https://webdav.example.com");
  });

  it("has loadSettings function", async () => {
    const { useWebDAVSettings } = await import("./useWebDAVSettings");
    const { result } = renderHook(() => useWebDAVSettings());
    
    expect(typeof result.current.loadSettings).toBe("function");
  });
});
