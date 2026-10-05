import { StrictMode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t } from "@/i18n";
import { AppFilterTab } from "./AppFilterTab";

beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_running_apps") return [];
    return null;
  });
});

afterEach(() => vi.restoreAllMocks());

describe("application filter failure feedback", () => {
  it("rolls back a failed monitor-type change and shows exactly one user error in StrictMode", async () => {
    render(<StrictMode><AppFilterTab /><Toaster /></StrictMode>);
    await act(async () => {});
    const button = screen.getByRole("button", { name: t("settings.appFilter.plainText"), pressed: true });
    const rejectSaves: ((error: unknown) => void)[] = [];
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "set_setting") return new Promise((_, reject) => { rejectSaves.push(reject); });
      return Promise.resolve(null);
    });
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-pressed", "false");
    await act(async () => {
      for (const reject of rejectSaves) reject({ code: "permission_denied", detail: "private path" });
    });
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.permission_denied"));
    expect(console.error).toHaveBeenCalledTimes(1);
  });

  it("does not show a misleading error toast when background application loading fails", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("offline"));
    render(<><AppFilterTab /><Toaster /></>);
    await waitFor(() => expect(console.error).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("settings.appFilter.plainText"), pressed: true })).toBeInTheDocument();
  });
});
