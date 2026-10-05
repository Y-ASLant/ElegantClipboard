import { createElement } from "react";
import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t, useLocaleStore, type Locale } from "@/i18n";
import { logError } from "./logger";
import { getOperationErrorMessage, reportOperationError, reportUserError, runClipboardOperation, type OperationResult } from "./operation-feedback";

beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.mocked(invoke).mockReset();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("operation feedback", () => {
  it.each<Locale>(["zh-CN", "en", "zh-TW"])("shows a localized native reason without diagnostic secrets in %s", (locale) => {
    useLocaleStore.setState({ locale });
    render(createElement(Toaster));
    act(() => reportOperationError("copy", { code: "resource_missing", detail: "https://user:secret@example.com/private" }));
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.resource_missing"));
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.operations.copy"));
    expect(screen.getByRole("alert").textContent).not.toContain("secret");
    expect(screen.getByRole("alert").textContent).not.toContain("operationFeedback.");
    expect(console.error).toHaveBeenCalledTimes(1);
  });

  it.each([
    new Error("password=secret\n at private.ts:42"),
    "resource_missing: password=secret",
    { code: "resource_missing", detail: { password: "secret" } },
    { code: "unknown", detail: "password=secret" },
    { code: "toString", detail: "password=secret" },
    null,
  ])("keeps malformed and legacy diagnostics out of user feedback: %j", (error) => {
    expect(getOperationErrorMessage(error)).toBe(t("operationFeedback.unknownReason"));
    render(createElement(Toaster));
    act(() => reportUserError(t("operationFeedback.userActions.saveSettings"), error));
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.unknownReason"));
    expect(screen.getByRole("alert").textContent).not.toContain("secret");
  });

  it("does not notify users about background diagnostic errors", () => {
    render(createElement(Toaster));
    act(() => logError("Failed to refresh background state", new Error("offline")));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("treats a dismissed save dialog as cancellation with no error or success notice", async () => {
    render(createElement(Toaster));
    vi.mocked(invoke).mockResolvedValue(false);
    let result: OperationResult<boolean> | undefined;
    await act(async () => { result = await runClipboardOperation<boolean>("save", { id: 1 }); });
    expect(result).toEqual({ status: "cancelled" });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(console.error).not.toHaveBeenCalled();
  });

  it.each(["pin", "favorite"] as const)("keeps a false %s value as a successful state change", async (operation) => {
    vi.mocked(invoke).mockResolvedValue(false);
    expect(await runClipboardOperation<boolean>(operation, { id: 1 })).toEqual({ status: "success", value: false });
    expect(console.error).not.toHaveBeenCalled();
  });

  it("reports a rejected native operation exactly once and returns failure", async () => {
    render(createElement(Toaster));
    vi.mocked(invoke).mockRejectedValue({ code: "permission_denied", detail: "private path" });
    let result: OperationResult | undefined;
    await act(async () => { result = await runClipboardOperation("paste", { id: 1 }); });
    expect(result).toEqual({ status: "failed" });
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.reasons.permission_denied"));
    expect(console.error).toHaveBeenCalledTimes(1);
  });
});
