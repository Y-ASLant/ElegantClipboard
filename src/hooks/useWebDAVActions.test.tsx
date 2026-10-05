import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t } from "@/i18n";
import { resetWebDAVSyncStoreForTests, useWebDAVSyncStore } from "@/stores/webdav-sync";
import { useWebDAVActions } from "./useWebDAVActions";

vi.mock("@/stores/clipboard", () => ({
  useClipboardStore: { getState: () => ({ refresh: async () => {} }) },
}));
vi.mock("@/stores/ui-settings", () => ({ loadUISettingsFromBackend: async () => {} }));

beforeEach(() => {
  resetWebDAVSyncStoreForTests();
  vi.mocked(invoke).mockReset();
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("WebDAV operation state and failure ownership", () => {
  it("keeps an upload busy until the accepted request completes and tracks remaining media", async () => {
    let complete: ((value: { message: string; pending_media_workers: number }) => void) | undefined;
    const accepted = new Promise<{ message: string; pending_media_workers: number }>((resolve) => { complete = resolve; });
    let requests = 0;
    vi.mocked(invoke).mockImplementation(() => {
      requests += 1;
      return requests === 1 ? accepted : Promise.resolve({ message: "duplicate", pending_media_workers: 0 });
    });
    const { result } = renderHook(() => useWebDAVActions());
    let upload: Promise<void> | undefined;
    act(() => { upload = result.current.handleUpload(); });
    expect(result.current.syncing).toBe(true);
    await act(async () => result.current.handleUpload());
    expect(result.current.syncing).toBe(true);
    await act(async () => {
      complete!({ message: "metadata synchronized", pending_media_workers: 3 });
      await upload;
    });
    expect(result.current.syncing).toBe(false);
    expect(result.current.pendingMediaWorkers).toBe(3);
    expect(result.current.statusType).toBe("success");
  });

  it("shows a controlled inline failure without a duplicate toast or private native detail", async () => {
    render(<Toaster />);
    const { result } = renderHook(() => useWebDAVActions());
    vi.mocked(invoke).mockRejectedValueOnce({ code: "permission_denied", detail: "private token and local path" });
    await act(async () => result.current.handleTestConnection());
    expect(result.current.testing).toBe(false);
    expect(result.current.statusType).toBe("error");
    expect(result.current.statusMsg).toBe(t("operationFeedback.reasons.permission_denied"));
    expect(result.current.statusMsg).not.toContain("private");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    vi.mocked(invoke).mockResolvedValueOnce("connected");
    await act(async () => result.current.handleTestConnection());
    expect(result.current.statusType).toBe("success");
    expect(result.current.testing).toBe(false);
  });

  it("clears stale pending media when a download fails without publishing success", async () => {
    useWebDAVSyncStore.setState({ pendingMediaWorkers: 4, statusType: "success" });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("secret upstream response"));
    const { result } = renderHook(() => useWebDAVActions());
    await act(async () => result.current.handleDownload());
    expect(result.current.syncing).toBe(false);
    expect(result.current.pendingMediaWorkers).toBe(0);
    expect(result.current.statusType).toBe("error");
    expect(result.current.statusMsg).toBe(t("operationFeedback.unknownReason"));
    expect(result.current.statusMsg).not.toContain("secret");
  });
});
