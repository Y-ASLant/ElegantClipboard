import { invoke } from "@tauri-apps/api/core";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { t, useLocaleStore } from "@/i18n";

describe("i18n", () => {
  beforeEach(() => {
    useLocaleStore.setState({ locale: "zh-CN", loaded: true });
    vi.mocked(invoke).mockReset();
  });

  it("defaults to Simplified Chinese", () => {
    expect(t("groups.all")).toBe("全部");
  });

  it("switches to English", () => {
    useLocaleStore.setState({ locale: "en" });
    expect(t("groups.all")).toBe("All");
    expect(t("settings.title")).toBe("Settings");
  });

  it("switches to Traditional Chinese", () => {
    useLocaleStore.setState({ locale: "zh-TW" });
    expect(t("groups.text")).toBe("文字");
    expect(t("groups.other")).toBe("其他");
  });

  it("interpolates params", () => {
    expect(t("app.batchSelected", { count: 3 })).toBe("已选择 3 项");
  });

  it("rolls back both visible language and document language when persistence fails, and rejects to the UI owner", async () => {
    const error = { code: "permission_denied", detail: "private settings path" };
    vi.mocked(invoke).mockRejectedValueOnce(error);
    const save = useLocaleStore.getState().setLocale("en");
    expect(t("settings.title")).toBe("Settings");
    expect(document.documentElement.lang).toBe("en");
    await expect(save).rejects.toBe(error);
    expect(t("settings.title")).toBe("设置");
    expect(document.documentElement.lang).toBe("zh-CN");
  });

  it("keeps a successfully saved language visible", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await useLocaleStore.getState().setLocale("zh-TW");
    expect(t("settings.title")).toBe("設定");
    expect(document.documentElement.lang).toBe("zh-TW");
  });
});
