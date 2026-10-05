import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { t } from "@/i18n";
import { useClipboardStore, type ClipboardItem } from "@/stores/clipboard";
import { useUISettings } from "@/stores/ui-settings";
import { ClipboardItemCard } from "./ClipboardItemCard";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => path,
}));
const originalPath = "E:\\下载\\omp.exe";
const stagedPath = "E:\\clipboard\\staged\\omp.exe";
const item: ClipboardItem = {
  id: 7, content_type: "files", file_paths: JSON.stringify([originalPath]),
  byte_size: 234392576, preview: "omp.exe", content_hash: "file-hash",
  text_content: null, html_content: null, rtf_content: null, image_path: null,
  image_width: null, image_height: null, is_pinned: false, is_favorite: false,
  favorite_order: 0, sort_order: 0, created_at: "2026-10-03T11:17:00Z",
  updated_at: "2026-10-03T11:17:00Z", access_count: 0, last_accessed_at: null,
  char_count: null, source_app_name: null, source_app_icon: null, group_id: null,
};
let exists: boolean;
let resolvedPath: string;
let onWindowShown: EventCallback<unknown> | undefined;
let tooLarge: boolean;

function renderCard(overrides: Partial<ClipboardItem> = {}) {
  return render(<TooltipProvider><ClipboardItemCard item={{ ...item, ...overrides }} /><Toaster /></TooltipProvider>);
}

async function openMenu() {
  fireEvent.contextMenu(screen.getByText("omp.exe"));
  return screen.findByRole("menuitem", { name: t("clipboard.contextMenu.saveAs") });
}

async function selectFileMenu(key: string) {
  await openMenu();
  const action = screen.getByRole("menuitem", { name: t(`clipboard.contextMenu.${key}`) });
  await waitFor(() => expect(action).not.toHaveAttribute("aria-disabled", "true"));
  fireEvent.keyDown(action, { key: "Enter" });
}

beforeEach(() => {
  exists = true;
  resolvedPath = originalPath;
  tooLarge = true;
  onWindowShown = undefined;
  vi.mocked(invoke).mockReset();
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    if (event === "window-shown") onWindowShown = handler;
    return () => {};
  });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "batch_get_item_file_status") {
      return { 7: { all_exist: exists, clipboard_usable: exists, resolved_paths: [resolvedPath],
        checks: { [originalPath]: { exists, is_dir: false } }, too_large: tooLarge } };
    }
    if (command === "check_files_exist") {
      return { [resolvedPath]: { exists, is_dir: false } };
    }
    return undefined;
  });
  useClipboardStore.setState({ searchQuery: "", batchMode: false, selectedIds: new Set() });
  useUISettings.setState({ showSourceApp: false, showDragAreaIndicator: false, pasteMoveToTop: false });
});

afterEach(cleanup);
afterEach(() => sessionStorage.clear());

describe("file card validity", () => {
  it("marks a missing oversized file invalid and disables file operations", async () => {
    exists = false;
    renderCard();
    expect(await screen.findByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.fileTooLarge"))).not.toBeInTheDocument();
    const save = await openMenu();
    expect(save).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("menuitem", { name: t("clipboard.contextMenu.showInExplorer") }))
      .toHaveAttribute("aria-disabled", "true");
  });

  it("pastes missing original paths as text while file content remains unavailable", async () => {
    exists = false;
    renderCard();
    await screen.findByText(t("cardContent.invalid"));
    await openMenu();
    const pastePath = screen.getByRole("menuitem", { name: t("clipboard.contextMenu.pasteAsPath") });
    expect(pastePath).not.toHaveAttribute("aria-disabled", "true");
    fireEvent.click(pastePath);
    expect(await screen.findByRole("status", { name: t("operationFeedback.pasteSucceeded") })).toBeInTheDocument();
    expect(screen.getByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("refreshes a previously valid file when the window is shown and recovers when restored", async () => {
    renderCard();
    await waitFor(() => expect(onWindowShown).toBeDefined());
    await screen.findByText(t("cardContent.fileTooLarge"));
    exists = false;
    act(() => onWindowShown!({ event: "window-shown", id: 1, payload: null }));
    expect(await screen.findByText(t("cardContent.invalid"))).toBeInTheDocument();
    exists = true;
    act(() => onWindowShown!({ event: "window-shown", id: 1, payload: null }));
    expect(await screen.findByText(t("cardContent.fileTooLarge"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
  });

  it("detects a moved file on right click without a clipboard update", async () => {
    renderCard();
    await screen.findByText(t("cardContent.fileTooLarge"));
    exists = false;
    const save = await openMenu();
    expect(await screen.findByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(save).toHaveAttribute("aria-disabled", "true");
  });

  it("keeps a staged fallback usable but reports fresh save failure without success", async () => {
    resolvedPath = stagedPath;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "batch_get_item_file_status") return {
        7: { all_exist: true, clipboard_usable: true, resolved_paths: [stagedPath], checks: {}, too_large: true },
      };
      if (command === "save_file_as") throw { code: "resource_missing", detail: "staged source removed" };
      return undefined;
    });
    renderCard();
    await screen.findByText(stagedPath);
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
    const save = await openMenu();
    await waitFor(() => expect(save).not.toHaveAttribute("aria-disabled", "true"));
    fireEvent.click(save);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(t("operationFeedback.reasons.resource_missing"));
    expect(alert).not.toHaveTextContent("staged source removed");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps an existing image file usable when its preview cannot be decoded", async () => {
    resolvedPath = "E:\\images\\broken.png";
    tooLarge = false;
    renderCard({ file_paths: JSON.stringify([resolvedPath]), byte_size: 128 });
    fireEvent.error(await screen.findByRole("img"));
    expect(await screen.findByText(t("cardContent.previewLoadFailed"))).toBeInTheDocument();
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
    fireEvent.contextMenu(screen.getByText("broken.png"));
    await waitFor(() => expect(screen.getByRole("menuitem", { name: t("clipboard.contextMenu.saveAs") }))
      .not.toHaveAttribute("aria-disabled", "true"));
  });

  it("marks a missing cached image unavailable and disables its content operations", async () => {
    resolvedPath = "E:\\images\\missing.png";
    exists = false;
    renderCard({ content_type: "image", image_path: resolvedPath, file_paths: null });
    expect(await screen.findByText(t("cardContent.invalid"))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("clipboard.copy") })).toBeDisabled();
    fireEvent.click(screen.getByText(t("cardContent.invalid")));
    expect(invoke).not.toHaveBeenCalledWith("paste_content", expect.anything());
    fireEvent.contextMenu(screen.getByText(t("cardContent.invalid")));
    for (const key of ["paste", "copy", "saveAs", "showInExplorer"]) {
      expect(await screen.findByRole("menuitem", { name: t(`clipboard.contextMenu.${key}`) }))
        .toHaveAttribute("aria-disabled", "true");
    }
  });

  it("does not show copy success while the clipboard request is pending", async () => {
    let complete: (() => void) | undefined;
    const pending = new Promise<void>((resolve) => { complete = resolve; });
    vi.mocked(invoke).mockReturnValueOnce(pending);
    renderCard({ content_type: "text", file_paths: null, text_content: "copy me", preview: "copy me" });
    fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
    expect(screen.queryByRole("status", { name: t("operationFeedback.copySucceeded") })).not.toBeInTheDocument();
    await act(async () => complete!());
    expect(await screen.findByRole("status", { name: t("operationFeedback.copySucceeded") })).toBeInTheDocument();
  });

  it("does not show copy success after the clipboard rejects the request", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ code: "clipboard_unavailable", detail: "clipboard busy" });
    const diagnostic = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      renderCard({ content_type: "text", file_paths: null, text_content: "copy me", preview: "copy me" });
      fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
      await screen.findByRole("alert");
      expect(screen.queryByRole("status", { name: t("operationFeedback.copySucceeded") })).not.toBeInTheDocument();
    } finally {
      diagnostic.mockRestore();
    }
  });

  it("ignores an old copy completion after the card is reused for another source", async () => {
    let complete!: () => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise<void>((resolve) => { complete = resolve; }));
    const original = { ...item, content_type: "text" as const, file_paths: null, text_content: "first", preview: "first" };
    const view = render(<TooltipProvider><ClipboardItemCard item={original} /><Toaster /></TooltipProvider>);
    fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
    view.rerender(<TooltipProvider><ClipboardItemCard item={{ ...original, content_hash: "changed", text_content: "second", preview: "second" }} /><Toaster /></TooltipProvider>);
    await act(async () => complete());
    expect(screen.getByText("second")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("ignores an earlier success after a later copy request failed", async () => {
    let complete!: () => void;
    vi.mocked(invoke)
      .mockReturnValueOnce(new Promise<void>((resolve) => { complete = resolve; }))
      .mockRejectedValueOnce({ code: "clipboard_unavailable", detail: "clipboard busy" });
    renderCard({ content_type: "text", file_paths: null, text_content: "copy me", preview: "copy me" });
    fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
    fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
    await screen.findByRole("alert");
    await act(async () => complete());
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("shows paste success even if the subsequent list refresh fails", async () => {
    useUISettings.setState({ pasteMoveToTop: true });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_clipboard_items") throw { code: "internal", detail: "refresh failed" };
      return undefined;
    });
    renderCard({ content_type: "text", file_paths: null, text_content: "paste me", preview: "paste me" });
    fireEvent.click(screen.getByText("paste me"));
    expect(await screen.findByRole("status", { name: t("operationFeedback.pasteSucceeded") })).toBeInTheDocument();
    expect(await screen.findByRole("alert")).not.toHaveTextContent("refresh failed");
  });

  it("does not show paste success while pending or after failure", async () => {
    let reject!: (error: unknown) => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise<void>((_, fail) => { reject = fail; }));
    renderCard({ content_type: "text", file_paths: null, text_content: "paste me", preview: "paste me" });
    fireEvent.click(screen.getByText("paste me"));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    await act(async () => reject({ code: "paste_failed", detail: "target rejected" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.paste_failed"));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("does not publish a completed copy after the card unmounts", async () => {
    let complete!: () => void;
    vi.mocked(invoke).mockReturnValueOnce(new Promise<void>((resolve) => { complete = resolve; }));
    const view = renderCard({ content_type: "text", file_paths: null, text_content: "copy me", preview: "copy me" });
    fireEvent.click(screen.getByRole("button", { name: t("clipboard.copy") }));
    view.unmount();
    renderCard({ id: 8, content_type: "text", file_paths: null, text_content: "new card", preview: "new card" });
    await act(async () => complete());
    expect(screen.getByText("new card")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("uses the same failure feedback for plain-text paste without success", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({ code: "paste_failed", detail: "target rejected" });
    renderCard({ content_type: "text", file_paths: null, text_content: "plain text", preview: "plain text" });
    fireEvent.contextMenu(screen.getByText("plain text"));
    fireEvent.click(await screen.findByRole("menuitem", { name: t("clipboard.contextMenu.pastePlainText") }));
    expect(await screen.findByRole("alert")).toHaveTextContent(t("operationFeedback.operations.pastePlain"));
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it.each([
    { command: "paste_as_path", key: "pasteAsPath", operation: "pastePath", code: "invalid_content" },
    { command: "show_in_explorer", key: "showInExplorer", operation: "showInExplorer", code: "resource_missing" },
    { command: "get_item_file_status", key: "viewDetails", operation: "details", code: "resource_missing" },
  ])("reports authoritative $operation failure even when the preview was usable", async ({ command, key, operation, code }) => {
    const normalInvoke = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (invokedCommand, args) => {
      if (invokedCommand === command) throw { code, detail: "resource changed after preview" };
      return normalInvoke(invokedCommand, args);
    });
    renderCard();
    await screen.findByText(t("cardContent.fileTooLarge"));
    await selectFileMenu(key);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(t(`operationFeedback.operations.${operation}`));
    expect(alert).toHaveTextContent(t(`operationFeedback.reasons.${code}`));
    expect(alert).not.toHaveTextContent("resource changed after preview");
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens file details only when the native status request succeeds", async () => {
    let complete!: (status: unknown) => void;
    const pending = new Promise<unknown>((resolve) => { complete = resolve; });
    const normalInvoke = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_item_file_status") return pending;
      return normalInvoke(command, args);
    });
    renderCard();
    await screen.findByText(t("cardContent.fileTooLarge"));
    await selectFileMenu("viewDetails");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await act(async () => complete({ all_exist: false, clipboard_usable: false, resolved_paths: [originalPath], checks: { [originalPath]: { exists: false, is_dir: false } } }));
    expect(await screen.findByRole("dialog")).toHaveTextContent(t("clipboard.filePartialInvalid"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not report or animate a cancelled save", async () => {
    const normalInvoke = vi.mocked(invoke).getMockImplementation()!;
    let cancelled = false;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "save_file_as") { cancelled = true; return false; }
      return normalInvoke(command, args);
    });
    renderCard();
    await screen.findByText(t("cardContent.fileTooLarge"));
    await selectFileMenu("saveAs");
    await waitFor(() => expect(cancelled).toBe(true));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps disk operations usable when only clipboard companion formats are unsupported", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "batch_get_item_file_status") return {
        7: { all_exist: true, clipboard_usable: false, resolved_paths: [originalPath], checks: { [originalPath]: { exists: true, is_dir: false } }, too_large: true },
      };
      return undefined;
    });
    renderCard();
    await screen.findByText(t("cardContent.fileTooLarge"));
    expect(screen.queryByText(t("cardContent.invalid"))).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("clipboard.copy") })).toBeDisabled();
    await openMenu();
    await waitFor(() => expect(screen.getByRole("menuitem", { name: t("clipboard.contextMenu.saveAs") })).not.toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByRole("menuitem", { name: t("clipboard.contextMenu.showInExplorer") })).not.toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("menuitem", { name: t("clipboard.contextMenu.paste") })).toHaveAttribute("aria-disabled", "true");
  });
});
