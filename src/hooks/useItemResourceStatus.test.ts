import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipboardItem } from "@/stores/clipboard";
import { useItemResourceStatus, type ItemFileStatus } from "./useItemResourceStatus";

vi.mock("@/lib/logger", () => ({ logError: vi.fn() }));

type ResourceItem = Pick<ClipboardItem, "id" | "content_type" | "file_paths" | "image_path" | "byte_size" | "content_hash">;
type ImageChecks = Record<string, { exists: boolean; is_dir: boolean }>;

const originalPath = "E:\\downloads\\image.png";
const stagedPath = "E:\\clipboard\\staged\\image.png";
const firstImage = "E:\\clipboard\\images\\first.png";
const secondImage = "E:\\clipboard\\images\\second.png";
const fileItem: ResourceItem = {
  id: 7,
  content_type: "files",
  file_paths: JSON.stringify([originalPath]),
  image_path: null,
  byte_size: 1024,
  content_hash: "file",
};
const imageItem: ResourceItem = {
  ...fileItem,
  content_type: "image",
  file_paths: null,
  image_path: firstImage,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function dispatchChecks() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(50);
  });
}

beforeEach(({ task }) => {
  fileItem.content_hash = `${task.name}-file`;
  imageItem.content_hash = `${task.name}-image`;
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset();
});

afterEach(async () => {
  cleanup();
  await act(async () => {
    await vi.runOnlyPendingTimersAsync();
  });
  vi.useRealTimers();
});

describe("useItemResourceStatus", () => {
  it("keeps an unavailable result across virtualized unmounts while rechecking", async () => {
    const pending = deferred<ImageChecks>();
    vi.mocked(invoke).mockResolvedValueOnce({
      [firstImage]: { exists: false, is_dir: false },
    }).mockReturnValueOnce(pending.promise);
    const first = renderHook(() => useItemResourceStatus(imageItem));
    await dispatchChecks();
    expect(first.result.current.availability).toBe("unavailable");
    first.unmount();

    const second = renderHook(() => useItemResourceStatus(imageItem));
    expect(second.result.current.availability).toBe("unavailable");
    expect(second.result.current.isChecking).toBe(true);
    expect(second.result.current.clipboardUsable).toBe(false);
    await dispatchChecks();
    await act(async () => pending.resolve({ [firstImage]: { exists: true, is_dir: false } }));
    expect(second.result.current.availability).toBe("available");
    expect(second.result.current.isChecking).toBe(false);
    expect(second.result.current.clipboardUsable).toBe(true);
  });

  it("keeps the last missing marker during refresh and replaces it on recovery", async () => {
    const pending = deferred<ImageChecks>();
    vi.mocked(invoke).mockResolvedValueOnce({
      [firstImage]: { exists: false, is_dir: false },
    }).mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useItemResourceStatus(imageItem));
    await dispatchChecks();
    act(() => result.current.refresh());
    expect(result.current.availability).toBe("unavailable");
    expect(result.current.clipboardUsable).toBe(false);
    await dispatchChecks();
    await act(async () => pending.resolve({ [firstImage]: { exists: true, is_dir: false } }));
    expect(result.current.availability).toBe("available");
  });

  it("never authorizes a new image path with the previous source's result", async () => {
    const first = deferred<ImageChecks>();
    const second = deferred<ImageChecks>();
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook((item) => useItemResourceStatus(item), {
      initialProps: imageItem,
    });
    await dispatchChecks();

    rerender({ ...imageItem, image_path: secondImage });
    expect(result.current.availability).toBe("checking");
    expect(result.current.paths).toEqual([secondImage]);
    await dispatchChecks();
    await act(async () => {
      first.resolve({ [firstImage]: { exists: true, is_dir: false } });
    });
    expect(result.current.availability).toBe("checking");
    expect(result.current.paths).toEqual([secondImage]);

    await act(async () => {
      second.resolve({ [secondImage]: { exists: false, is_dir: false } });
    });
    expect(result.current.availability).toBe("unavailable");
    expect(result.current.paths).toEqual([secondImage]);
  });

  it("keeps the latest refresh result when older checks finish later", async () => {
    const older = deferred<ImageChecks>();
    const newer = deferred<ImageChecks>();
    vi.mocked(invoke).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
    const { result } = renderHook(() => useItemResourceStatus(imageItem));
    await dispatchChecks();
    act(() => result.current.refresh());
    await dispatchChecks();

    await act(async () => {
      newer.resolve({ [firstImage]: { exists: false, is_dir: false } });
    });
    expect(result.current.availability).toBe("unavailable");
    await act(async () => {
      older.resolve({ [firstImage]: { exists: true, is_dir: false } });
    });
    expect(result.current.availability).toBe("unavailable");
  });

  it("clears authorization, staged paths and size metadata when a recheck fails", async () => {
    const recheck = deferred<Record<string, ItemFileStatus>>();
    vi.mocked(invoke).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: true, resolved_paths: [stagedPath], checks: {}, too_large: true },
    }).mockReturnValueOnce(recheck.promise);
    const { result } = renderHook(() => useItemResourceStatus(fileItem));
    await dispatchChecks();
    expect(result.current.availability).toBe("available");
    expect(result.current.paths).toEqual([stagedPath]);
    expect(result.current.originalPaths).toEqual([originalPath]);
    expect(result.current.tooLarge).toBe(true);

    act(() => result.current.refresh());
    expect(result.current.isChecking).toBe(true);
    expect(result.current.clipboardUsable).toBe(false);
    await dispatchChecks();
    await act(async () => recheck.reject(new Error("Native check failed")));
    expect(result.current.availability).toBe("unknown");
    expect(result.current.isChecking).toBe(false);
    expect(result.current.clipboardUsable).toBe(false);
    expect(result.current.paths).toEqual([originalPath]);
    expect(result.current.tooLarge).toBe(false);
  });

  it("keeps virtual-file clipboard entries available without disk paths", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: true, resolved_paths: [], checks: {} },
    });
    const { result } = renderHook(() => useItemResourceStatus({ ...fileItem, file_paths: "[]" }));
    await dispatchChecks();
    expect(result.current.availability).toBe("available");
    expect(result.current.paths).toEqual([]);
    expect(result.current.originalPaths).toEqual([]);
    expect(result.current.tooLarge).toBe(false);
  });

  it("keeps real file availability independent of unsupported clipboard formats", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: false, resolved_paths: [originalPath], checks: { [originalPath]: { exists: true, is_dir: false } } },
    });
    const { result } = renderHook(() => useItemResourceStatus(fileItem));
    await dispatchChecks();
    expect(result.current.availability).toBe("available");
    expect(result.current.clipboardUsable).toBe(false);
    expect(result.current.paths).toEqual([originalPath]);
  });

  it("refreshes size metadata for the same item's updated byte size", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: true, resolved_paths: [originalPath], checks: {}, too_large: false },
    }).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: true, resolved_paths: [originalPath], checks: {}, too_large: true },
    });
    const { result, rerender } = renderHook((item) => useItemResourceStatus(item), {
      initialProps: fileItem,
    });
    await dispatchChecks();
    expect(result.current.availability).toBe("available");
    expect(result.current.tooLarge).toBe(false);
    rerender({ ...fileItem, byte_size: 100_000_000 });
    expect(result.current.availability).toBe("checking");
    await dispatchChecks();
    expect(result.current.availability).toBe("available");
    expect(result.current.tooLarge).toBe(true);
  });

  it("does not reuse display status when the same id points to different content", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      [firstImage]: { exists: false, is_dir: false },
    });
    const first = renderHook(() => useItemResourceStatus(imageItem));
    await dispatchChecks();
    first.unmount();
    const second = renderHook(() => useItemResourceStatus({ ...imageItem, content_hash: "replacement" }));
    expect(second.result.current.availability).toBe("checking");
    expect(second.result.current.clipboardUsable).toBe(false);
    second.unmount();
    vi.mocked(invoke).mockResolvedValue({});
  });

  it("preserves staged-path presentation but blocks cached authorization on remount", async () => {
    const pending = deferred<Record<string, ItemFileStatus>>();
    vi.mocked(invoke).mockResolvedValueOnce({
      7: { all_exist: true, clipboard_usable: true, resolved_paths: [stagedPath], checks: {}, too_large: true },
    }).mockReturnValueOnce(pending.promise);
    const first = renderHook(() => useItemResourceStatus(fileItem));
    await dispatchChecks();
    first.unmount();
    const second = renderHook(() => useItemResourceStatus(fileItem));
    expect(second.result.current.availability).toBe("available");
    expect(second.result.current.paths).toEqual([stagedPath]);
    expect(second.result.current.tooLarge).toBe(true);
    expect(second.result.current.isChecking).toBe(true);
    expect(second.result.current.clipboardUsable).toBe(false);
    await dispatchChecks();
    await act(async () => pending.reject(new Error("Disk unavailable")));
    expect(second.result.current.availability).toBe("unknown");
    second.unmount();
    vi.mocked(invoke).mockResolvedValue({});
    const third = renderHook(() => useItemResourceStatus(fileItem));
    expect(third.result.current.availability).toBe("checking");
    expect(third.result.current.paths).toEqual([originalPath]);
    third.unmount();
  });

  it("exposes resource overlays as unknown and in-memory content as available", () => {
    const { result, rerender } = renderHook((item) => useItemResourceStatus(item, false), {
      initialProps: imageItem,
    });
    expect(result.current.availability).toBe("unknown");
    act(() => result.current.refresh());
    expect(result.current.availability).toBe("unknown");
    rerender({ ...imageItem, content_type: "url", image_path: null });
    expect(result.current.availability).toBe("available");
    expect(result.current.paths).toEqual([]);
  });
});
