import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, type Window as TauriWindow } from "@tauri-apps/api/window";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t } from "@/i18n";
import { TextEditor } from "./TextEditor";

vi.mock("@/lib/theme-applier", () => ({ initTheme: vi.fn(() => Promise.resolve()) }));

const close = vi.fn(() => Promise.resolve());
let editResult: () => Promise<boolean>;

beforeEach(() => {
  close.mockClear();
  editResult = () => Promise.resolve(false);
  window.history.replaceState(null, "", "/text-editor.html?id=1");
  vi.mocked(getCurrentWindow).mockReturnValue({
    show: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
    close,
  } as unknown as TauriWindow);
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_clipboard_item") return { text_content: "Original text" };
    if (command === "update_text_content") return editResult();
    return undefined;
  });
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => vi.restoreAllMocks());

describe("text editor operation outcomes", () => {
  it("retains unsaved text and stays open after a rejected save-and-close", async () => {
    render(<><TextEditor /><Toaster /></>);
    const editor = await screen.findByRole("textbox");
    await waitFor(() => expect(editor).toHaveValue("Original text"));
    fireEvent.change(editor, { target: { value: "Changed text" } });
    editResult = () => Promise.reject({ code: "permission_denied", detail: "password=secret" });
    fireEvent.click(screen.getByRole("button", { name: t("textEditor.saveAndClose") }));
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.permission_denied")));
    expect(editor).toHaveValue("Changed text");
    expect(screen.getByText(t("textEditor.unsaved"))).toBeInTheDocument();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert").textContent).not.toContain("secret");
    expect(close).not.toHaveBeenCalled();
  });

  it("keeps the editor dirty while saving, then treats false as a successful edit rather than cancellation", async () => {
    render(<TextEditor />);
    const editor = await screen.findByRole("textbox");
    await waitFor(() => expect(editor).toHaveValue("Original text"));
    fireEvent.change(editor, { target: { value: "Changed text" } });
    let resolveSave!: (deleted: boolean) => void;
    editResult = () => new Promise<boolean>((resolve) => { resolveSave = resolve; });
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    expect(screen.getByText(t("textEditor.unsaved"))).toBeInTheDocument();
    expect(close).not.toHaveBeenCalled();
    await act(async () => { resolveSave(false); });
    await waitFor(() => expect(screen.queryByText(t("textEditor.unsaved"))).not.toBeInTheDocument());
    expect(editor).toHaveValue("Changed text");
    expect(close).not.toHaveBeenCalled();
  });

  it("closes exactly once after the backend confirms an empty edit deleted the item", async () => {
    render(<TextEditor />);
    const editor = await screen.findByRole("textbox");
    await waitFor(() => expect(editor).toHaveValue("Original text"));
    fireEvent.change(editor, { target: { value: "" } });
    editResult = () => Promise.resolve(true);
    fireEvent.click(screen.getByRole("button", { name: t("textEditor.saveAndClose") }));
    await waitFor(() => expect(close).toHaveBeenCalledTimes(1));
  });
});
