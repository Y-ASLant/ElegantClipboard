import { describe, it, expect, beforeEach } from "vitest";
import { useUISettings } from "./ui-settings";

// Reset store before each test
beforeEach(() => {
  useUISettings.setState({
    cardMaxLines: 3,
    showTime: true,
    showCharCount: true,
    showByteSize: true,
    showSourceApp: true,
    sourceAppDisplay: "both",
    imagePreviewEnabled: true,
    textPreviewEnabled: true,
    previewUnboundedMode: false,
    previewZoomStep: 15,
    previewPosition: "auto",
    imageAutoHeight: true,
    imageMaxHeight: 512,
    showImageFileName: true,
    colorTheme: "system",
    sharpCorners: false,
    autoResetState: false,
    keyboardNavigation: false,
    searchAutoFocus: false,
    searchAutoClear: true,
    darkMode: "auto",
    cardDensity: "standard",
    timeFormat: "absolute",
    hoverPreviewDelay: 150,
    copySound: false,
    copySoundTiming: "immediate",
    pasteSound: false,
    pasteSoundTiming: "immediate",
    pasteCloseWindow: true,
    pasteMoveToTop: false,
    showCategoryFilter: true,
    showDragAreaIndicator: true,
    windowAnimation: false,
    windowEffect: "none",
    toolbarButtons: ["clear", "batch", "pin", "settings"],
    customFont: "",
    uiFontSize: 14,
    cardFont: "",
    cardFontSize: 14,
    previewFont: "",
    previewFontSize: 13,
    onboardingCompleted: false,
  });
});

describe("ui-settings store", () => {
  describe("initial state", () => {
    it("has correct defaults", () => {
      const state = useUISettings.getState();
      expect(state.cardMaxLines).toBe(3);
      expect(state.colorTheme).toBe("system");
      expect(state.windowEffect).toBe("none");
      expect(state.toolbarButtons).toEqual(["clear", "batch", "pin", "settings"]);
    });
  });

  describe("makeSetter pattern", () => {
    it("setCardMaxLines updates value", () => {
      useUISettings.getState().setCardMaxLines(5);
      expect(useUISettings.getState().cardMaxLines).toBe(5);
    });

    it("setColorTheme updates value", () => {
      useUISettings.getState().setColorTheme("emerald");
      expect(useUISettings.getState().colorTheme).toBe("emerald");
    });

    it("setShowTime toggles boolean", () => {
      useUISettings.getState().setShowTime(false);
      expect(useUISettings.getState().showTime).toBe(false);
    });

    it("setCardDensity updates enum", () => {
      useUISettings.getState().setCardDensity("compact");
      expect(useUISettings.getState().cardDensity).toBe("compact");
    });

    it("setHoverPreviewDelay updates number", () => {
      useUISettings.getState().setHoverPreviewDelay(300);
      expect(useUISettings.getState().hoverPreviewDelay).toBe(300);
    });
  });

  describe("setToolbarButtons", () => {
    it("updates toolbar buttons array", () => {
      const newButtons = ["clear", "settings"] as const;
      useUISettings.getState().setToolbarButtons([...newButtons]);
      expect(useUISettings.getState().toolbarButtons).toEqual([...newButtons]);
    });
  });

  describe("setWindowEffect", () => {
    it("updates window effect", () => {
      useUISettings.getState().setWindowEffect("mica");
      expect(useUISettings.getState().windowEffect).toBe("mica");
    });
  });

  describe("resetFontSettings", () => {
    it("resets all font settings to defaults", () => {
      useUISettings.getState().setCustomFont("Arial");
      useUISettings.getState().setUIFontSize(20);
      useUISettings.getState().setCardFont("Times New Roman");
      useUISettings.getState().setCardFontSize(18);

      useUISettings.getState().resetFontSettings();

      const state = useUISettings.getState();
      expect(state.customFont).toBe("");
      expect(state.uiFontSize).toBe(14);
      expect(state.cardFont).toBe("");
      expect(state.cardFontSize).toBe(14);
      expect(state.previewFont).toBe("");
      expect(state.previewFontSize).toBe(13);
    });
  });

  describe("setKeyboardNavigation", () => {
    it("updates keyboard navigation setting", () => {
      useUISettings.getState().setKeyboardNavigation(true);
      expect(useUISettings.getState().keyboardNavigation).toBe(true);
    });
  });

  describe("multiple rapid updates", () => {
    it("handles rapid sequential updates correctly", () => {
      const store = useUISettings.getState();
      store.setCardMaxLines(1);
      store.setCardMaxLines(2);
      store.setCardMaxLines(3);
      store.setCardMaxLines(4);
      store.setCardMaxLines(5);
      expect(useUISettings.getState().cardMaxLines).toBe(5);
    });
  });
});
