import { describe, it, expect, vi, beforeEach } from "vitest";
import { translateText, LANGUAGES, PROVIDER_OPTIONS } from "./translate";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string, args?: Record<string, unknown>) => {
    if (command === "translate_text") {
      return Promise.resolve("Translated text");
    }
    return Promise.resolve();
  }),
}));

vi.mock("@/lib/logger", () => ({
  logError: vi.fn(),
}));

const mockGetState = vi.fn().mockReturnValue({
  enabled: true,
  languageMode: "auto",
  sourceLanguage: null,
  targetLanguage: null,
  provider: "microsoft",
  proxyMode: false,
  proxyUrl: "",
  deeplxEndpoint: "",
  googleApiKey: "",
  baiduAppId: "",
  baiduSecretKey: "",
  openaiEndpoint: "",
  openaiApiKey: "",
  openaiModel: "",
});

vi.mock("@/stores/translate-settings", () => ({
  useTranslateSettings: {
    getState: (...args: unknown[]) => mockGetState(...args),
  },
}));

describe("translate", () => {
  describe("LANGUAGES", () => {
    it("has 13 languages", () => {
      expect(LANGUAGES).toHaveLength(13);
    });

    it("has Chinese", () => {
      expect(LANGUAGES.find((l) => l.value === "zh")).toBeDefined();
    });

    it("has English", () => {
      expect(LANGUAGES.find((l) => l.value === "en")).toBeDefined();
    });
  });

  describe("PROVIDER_OPTIONS", () => {
    it("has 6 providers", () => {
      expect(PROVIDER_OPTIONS).toHaveLength(6);
    });

    it("has Microsoft provider", () => {
      expect(PROVIDER_OPTIONS.find((p) => p.value === "microsoft")).toBeDefined();
    });

    it("has OpenAI provider", () => {
      expect(PROVIDER_OPTIONS.find((p) => p.value === "openai")).toBeDefined();
    });
  });

  describe("translateText", () => {
    beforeEach(() => {
      mockGetState.mockReturnValue({
        enabled: true,
        languageMode: "auto",
        sourceLanguage: null,
        targetLanguage: null,
        provider: "microsoft",
        proxyMode: false,
        proxyUrl: "",
        deeplxEndpoint: "",
        googleApiKey: "",
        baiduAppId: "",
        baiduSecretKey: "",
        openaiEndpoint: "",
        openaiApiKey: "",
        openaiModel: "",
      });
    });

    it("translates text successfully", async () => {
      const result = await translateText("Hello");
      expect(result).toBe("Translated text");
    });

    it("throws when disabled", async () => {
      mockGetState.mockReturnValue({
        ...mockGetState(),
        enabled: false,
      });
      await expect(translateText("Hello")).rejects.toThrow("翻译功能未启用");
    });

    it("uses manual language mode", async () => {
      mockGetState.mockReturnValue({
        ...mockGetState(),
        languageMode: "manual",
        sourceLanguage: "en",
        targetLanguage: "zh",
      });
      const result = await translateText("Hello");
      expect(result).toBe("Translated text");
    });
  });
});
