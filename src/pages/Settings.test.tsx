import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DataSettings } from "@/components/settings/DataTab";
import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { t } from "@/i18n";
import { Settings } from "./Settings";

vi.mock("@/components/settings/DataTab", () => ({
  DataTab: ({ settings, onSettingsChange }: {
    settings: DataSettings;
    onSettingsChange: (settings: DataSettings) => void;
  }) => (
    <>
      {(["max_history_count", "max_content_size_kb", "max_image_size_kb", "auto_cleanup_days"] as const).map((key) => (
        <input key={key} aria-label={key} value={settings[key]}
          onChange={(event) => onSettingsChange({ ...settings, [key]: Number(event.target.value) })} />
      ))}
    </>
  ),
}));

const initialValues: Record<string, string> = {
  max_history_count: "1000", max_content_size_kb: "1024",
  max_image_size_kb: "51200", auto_cleanup_days: "30",
};

function writes() {
  return vi.mocked(invoke).mock.calls.filter(([command]) =>
    command === "set_setting" || command === "enable_autostart" || command === "disable_autostart" ||
    command === "enable_admin_launch" || command === "disable_admin_launch",
  );
}

async function advance(ms: number) {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

async function renderSettings() {
  render(<TooltipProvider><Settings /><Toaster /></TooltipProvider>);
  await act(async () => {});
  await advance(1000);
  expect(writes()).toHaveLength(0);
  vi.mocked(invoke).mockClear();
}

function openData() {
  fireEvent.click(screen.getByRole("button", { name: t("settings.nav.data") }));
}

function changeField(key: string, value: string) {
  fireEvent.change(screen.getByLabelText(key), { target: { value } });
}

function settingSwitch(key: string) {
  const row = screen.getByText(t(key)).closest(".flex.items-center.justify-between")!;
  return within(row as HTMLElement).getByRole("switch");
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockReset().mockImplementation(async (command, args) => {
    if (command === "get_setting") return initialValues[(args as { key: string }).key] ?? null;
    if (command === "get_settings_batch") return {};
    if (command === "get_default_data_path") return "E:\\clipboard";
    if (command === "get_app_version") return "1.2.10";
    if (command === "get_build_time") return "2026-10-09";
    return false;
  });
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("settings changed-field persistence", () => {
  it.each(Object.keys(initialValues))("saves only the edited %s after 500ms", async (key) => {
    await renderSettings();
    openData();
    changeField(key, "200");
    await advance(499);
    expect(writes()).toHaveLength(0);
    await advance(1);
    expect(writes()).toEqual([["set_setting", { key, value: "200" }]]);
    changeField(key, initialValues[key]);
    changeField(key, "200");
    await advance(500);
    expect(writes()).toHaveLength(1);
  });

  it("does not save an edit reverted within the debounce window", async () => {
    await renderSettings();
    openData();
    changeField("max_history_count", "200");
    await advance(100);
    changeField("max_history_count", initialValues.max_history_count);
    await advance(500);
    expect(writes()).toHaveLength(0);
  });

  it("changes only the autostart command for its switch", async () => {
    await renderSettings();
    fireEvent.click(settingSwitch("settings.general.autoStart"));
    await advance(500);
    expect(writes()).toEqual([["enable_autostart"]]);
    fireEvent.click(settingSwitch("settings.general.autoStart"));
    await advance(500);
    expect(writes()).toEqual([["enable_autostart"], ["disable_autostart"]]);
  });

  it.each(["common.restartLater", "common.restartNow"])(
    "saves administrator preference once through %s",
    async (choice) => {
      await renderSettings();
      fireEvent.click(settingSwitch("settings.general.adminLaunch"));
      await act(async () => { fireEvent.click(screen.getByRole("button", { name: t(choice) })); });
      await advance(500);
      expect(writes()).toEqual([["enable_admin_launch"]]);
      expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "restart_app"))
        .toHaveLength(choice === "common.restartNow" ? 1 : 0);
      if (choice === "common.restartNow") return;
      fireEvent.click(screen.getByRole("button", { name: t("settings.nav.data") }));
      changeField("max_history_count", "200");
      await advance(500);
      expect(writes()).toEqual([["enable_admin_launch"], ["set_setting", { key: "max_history_count", value: "200" }]]);
    },
  );

  it("retries a failed field without rewriting successfully saved fields", async () => {
    await renderSettings();
    openData();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("write failed")).mockResolvedValue(undefined);
    changeField("max_history_count", "200");
    changeField("max_image_size_kb", "1024");
    await advance(500);
    expect(screen.getByRole("alert")).toHaveTextContent(t("operationFeedback.unknownReason"));
    changeField("auto_cleanup_days", "5");
    await advance(500);
    expect(writes()).toEqual([
      ["set_setting", { key: "max_history_count", value: "200" }],
      ["set_setting", { key: "max_image_size_kb", value: "1024" }],
      ["set_setting", { key: "max_history_count", value: "200" }],
      ["set_setting", { key: "auto_cleanup_days", value: "5" }],
    ]);
  });

  it("preserves a newer edit when the previous save is still pending", async () => {
    await renderSettings();
    openData();
    let finishSave!: () => void;
    vi.mocked(invoke).mockImplementationOnce(() => new Promise<void>((resolve) => { finishSave = resolve; }))
      .mockResolvedValue(undefined);
    changeField("max_history_count", "200");
    await advance(500);
    changeField("max_history_count", initialValues.max_history_count);
    await advance(500);
    await act(async () => { finishSave(); });
    expect(writes()).toEqual([
      ["set_setting", { key: "max_history_count", value: "200" }],
      ["set_setting", { key: "max_history_count", value: "1000" }],
    ]);
    changeField("max_image_size_kb", "1024");
    await advance(500);
    expect(writes()).toHaveLength(3);
    expect(writes()[2]).toEqual(["set_setting", { key: "max_image_size_kb", value: "1024" }]);
  });
});
