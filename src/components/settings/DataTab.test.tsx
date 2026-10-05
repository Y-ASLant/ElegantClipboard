import { invoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { t, useLocaleStore, type Locale } from "@/i18n";
import { DataTab, type DataSettings } from "./DataTab";

const settings: DataSettings = {
  data_path: "E:\\clipboard", max_history_count: 1000, max_content_size_kb: 1024,
  max_image_size_kb: 10240, auto_cleanup_days: 0,
};
let exportResult: () => Promise<string>;

beforeEach(() => {
  sessionStorage.clear();
  exportResult = () => Promise.resolve("Export completed");
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "export_data") return exportResult();
    return null;
  });
});

afterEach(() => vi.restoreAllMocks());

describe("data operation error ownership", () => {
  it.each<Locale>(["zh-CN", "en", "zh-TW"])("shows one safe localized inline export failure in %s", async (locale) => {
    useLocaleStore.setState({ locale });
    exportResult = () => Promise.reject("request unexpectedly cancelled; password=secret");
    render(<TooltipProvider><DataTab settings={settings} onSettingsChange={vi.fn()} /><Toaster /></TooltipProvider>);
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: t("settings.data.exportData") }));
    const error = await screen.findByRole("alert");
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(error).toHaveTextContent(t("operationFeedback.unknownReason"));
    expect(error).not.toHaveTextContent("secret");
    expect(error).toHaveClass("text-destructive");
    expect(console.error).toHaveBeenCalledTimes(1);
  });

  it("does not report the native export dialog cancellation as failure or success", async () => {
    exportResult = () => Promise.reject("用户取消了导出");
    render(<TooltipProvider><DataTab settings={settings} onSettingsChange={vi.fn()} /><Toaster /></TooltipProvider>);
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: t("settings.data.exportData") }));
    await waitFor(() => expect(screen.getByRole("button", { name: t("settings.data.exportData") })).toBeEnabled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(console.error).not.toHaveBeenCalled();
  });
});
