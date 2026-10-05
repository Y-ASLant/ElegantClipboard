import { createElement } from "react";
import { invoke } from "@tauri-apps/api/core";
import { act, render, screen } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { Toaster } from "@/components/ui/toast";
import { t } from "@/i18n";
import { translateText, getLanguages, getProviderOptions } from "./translate";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) => {
    if (command === "translate_text") {
      return Promise.resolve("Translated text");
    }
    return Promise.resolve();
  }),
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
  describe("getLanguages", () => {
    it("has 13 languages", () => {
      expect(getLanguages()).toHaveLength(13);
    });

    it("has Chinese", () => {
      expect(getLanguages().find((l) => l.value === "zh")).toBeDefined();
    });

    it("has English", () => {
      expect(getLanguages().find((l) => l.value === "en")).toBeDefined();
    });
  });

  describe("getProviderOptions", () => {
    it("has 6 providers", () => {
      expect(getProviderOptions()).toHaveLength(6);
    });

    it("has Microsoft provider", () => {
      expect(getProviderOptions().find((p) => p.value === "microsoft")).toBeDefined();
    });

    it("has OpenAI provider", () => {
      expect(getProviderOptions().find((p) => p.value === "openai")).toBeDefined();
    });
  });

  describe("translateText", () => {
    beforeEach(() => {
      vi.spyOn(console, "error").mockImplementation(() => {});
      vi.mocked(invoke).mockResolvedValue("Translated text");
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

    afterEach(() => vi.restoreAllMocks());

    it("translates text successfully", async () => {
      const result = await translateText("Hello");
      expect(result).toBe("Translated text");
    });

    it("throws when disabled", async () => {
      mockGetState.mockReturnValue({
        ...mockGetState(),
        enabled: false,
      });
      await expect(translateText("Hello")).rejects.toThrow(t("translate.errors.FEATURE_DISABLED"));
    });

    it("preserves localized provider errors without exposing their diagnostic details or showing a second notification", async () => {
      render(createElement(Toaster));
      vi.mocked(invoke).mockRejectedValue("TRANSLATE:FEATURE_DISABLED:password=secret");
      await act(async () => {
        await expect(translateText("Hello")).rejects.toThrow(t("translate.errors.FEATURE_DISABLED"));
      });
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      expect(console.error).toHaveBeenCalledTimes(1);
    });

    it.each(["password=secret", "TRANSLATE:UNKNOWN_CODE:password=secret", new Error("password=secret"), { credentials: "secret" }])(
      "localizes unrecognized translation failures safely: %j", async (error) => {
        vi.mocked(invoke).mockRejectedValue(error);
        await expect(translateText("Hello")).rejects.toThrow(t("operationFeedback.unknownReason"));
      },
    );

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
