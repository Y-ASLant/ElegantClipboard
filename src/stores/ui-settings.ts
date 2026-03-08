import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ColorTheme = "default" | "emerald" | "cyan" | "system";
export type DarkMode = "light" | "dark" | "auto";
export type CardDensity = "compact" | "standard" | "spacious";
export type TimeFormat = "relative" | "absolute";
export type WindowEffect = "none" | "mica" | "acrylic" | "tabbed";
export type SoundTiming = "immediate" | "after_success";
export type ToolbarButton = "clear" | "pin" | "batch" | "settings";

export const DEFAULT_TOOLBAR_BUTTONS: ToolbarButton[] = ["clear", "batch", "pin", "settings"];
export const MAX_TOOLBAR_BUTTONS = 5;

interface UISettings {
  cardMaxLines: number;
  showTime: boolean;
  showCharCount: boolean;
  showByteSize: boolean;
  showSourceApp: boolean;
  sourceAppDisplay: "both" | "name" | "icon";
  imagePreviewEnabled: boolean;
  textPreviewEnabled: boolean;
  previewUnboundedMode: boolean;
  previewZoomStep: number;
  previewPosition: "auto" | "left" | "right";
  imageAutoHeight: boolean;
  imageMaxHeight: number;
  showImageFileName: boolean;
  colorTheme: ColorTheme;
  sharpCorners: boolean;
  autoResetState: boolean;
  keyboardNavigation: boolean;
  searchAutoFocus: boolean;
  searchAutoClear: boolean;
  // New settings
  darkMode: DarkMode;
  cardDensity: CardDensity;
  timeFormat: TimeFormat;
  hoverPreviewDelay: number;
  copySound: boolean;
  copySoundTiming: SoundTiming;
  pasteSound: boolean;
  pasteSoundTiming: SoundTiming;
  pasteCloseWindow: boolean;
  pasteMoveToTop: boolean;
  showCategoryFilter: boolean;
  showDragAreaIndicator: boolean;
  windowAnimation: boolean;
  windowEffect: WindowEffect;
  toolbarButtons: ToolbarButton[];
  setCardMaxLines: (lines: number) => void;
  setShowTime: (show: boolean) => void;
  setShowCharCount: (show: boolean) => void;
  setShowByteSize: (show: boolean) => void;
  setShowSourceApp: (show: boolean) => void;
  setSourceAppDisplay: (mode: "both" | "name" | "icon") => void;
  setImagePreviewEnabled: (enabled: boolean) => void;
  setTextPreviewEnabled: (enabled: boolean) => void;
  setPreviewUnboundedMode: (enabled: boolean) => void;
  setPreviewZoomStep: (step: number) => void;
  setPreviewPosition: (pos: "auto" | "left" | "right") => void;
  setImageAutoHeight: (auto: boolean) => void;
  setImageMaxHeight: (height: number) => void;
  setShowImageFileName: (show: boolean) => void;
  setColorTheme: (theme: ColorTheme) => void;
  setSharpCorners: (enabled: boolean) => void;
  setAutoResetState: (enabled: boolean) => void;
  setKeyboardNavigation: (enabled: boolean) => void;
  setSearchAutoFocus: (enabled: boolean) => void;
  setSearchAutoClear: (enabled: boolean) => void;
  setDarkMode: (mode: DarkMode) => void;
  setCardDensity: (density: CardDensity) => void;
  setTimeFormat: (format: TimeFormat) => void;
  setHoverPreviewDelay: (delay: number) => void;
  setCopySound: (enabled: boolean) => void;
  setCopySoundTiming: (timing: SoundTiming) => void;
  setPasteSound: (enabled: boolean) => void;
  setPasteSoundTiming: (timing: SoundTiming) => void;
  setPasteCloseWindow: (enabled: boolean) => void;
  setPasteMoveToTop: (enabled: boolean) => void;
  setShowCategoryFilter: (enabled: boolean) => void;
  setShowDragAreaIndicator: (enabled: boolean) => void;
  setWindowAnimation: (enabled: boolean) => void;
  setWindowEffect: (effect: WindowEffect) => void;
  setToolbarButtons: (buttons: ToolbarButton[]) => void;
}

const STORAGE_KEY = "clipboard-ui-settings";
const SYNC_EVENT = "ui-settings-changed";

// Helper to broadcast settings change
const broadcastChange = (state: Partial<UISettings>) => {
  emit(SYNC_EVENT, state).catch(() => {});
};

export const useUISettings = create<UISettings>()(
  persist(
    (set) => {
      // Factory: creates a setter that updates state and broadcasts the change
      const makeSetter = <K extends keyof UISettings>(key: K) =>
        (value: UISettings[K]) => {
          set({ [key]: value } as unknown as Partial<UISettings>);
          broadcastChange({ [key]: value } as unknown as Partial<UISettings>);
        };

      return {
        cardMaxLines: 3,
        showTime: true,
        showCharCount: true,
        showByteSize: true,
        showSourceApp: true,
        sourceAppDisplay: "both" as "both" | "name" | "icon",
        imagePreviewEnabled: false,
        textPreviewEnabled: false,
        previewUnboundedMode: false,
        previewZoomStep: 15,
        previewPosition: "auto" as "auto" | "left" | "right",
        imageAutoHeight: true,
        imageMaxHeight: 512,
        showImageFileName: true,
        colorTheme: "system" as ColorTheme,
        sharpCorners: false,
        autoResetState: false,
        keyboardNavigation: false,
        searchAutoFocus: false,
        searchAutoClear: true,
        darkMode: "auto" as DarkMode,
        cardDensity: "standard" as CardDensity,
        timeFormat: "absolute" as TimeFormat,
        hoverPreviewDelay: 500,
        copySound: false,
        copySoundTiming: "immediate" as SoundTiming,
        pasteSound: false,
        pasteSoundTiming: "immediate" as SoundTiming,
        pasteCloseWindow: true,
        pasteMoveToTop: false,
        showCategoryFilter: true,
        showDragAreaIndicator: true,
        windowAnimation: false,
        windowEffect: "none" as WindowEffect,
        toolbarButtons: ["clear", "batch", "pin", "settings"] as ToolbarButton[],

        setCardMaxLines: makeSetter("cardMaxLines"),
        setShowTime: makeSetter("showTime"),
        setShowCharCount: makeSetter("showCharCount"),
        setShowByteSize: makeSetter("showByteSize"),
        setShowSourceApp: makeSetter("showSourceApp"),
        setSourceAppDisplay: makeSetter("sourceAppDisplay"),
        setImagePreviewEnabled: makeSetter("imagePreviewEnabled"),
        setTextPreviewEnabled: makeSetter("textPreviewEnabled"),
        setPreviewUnboundedMode: makeSetter("previewUnboundedMode"),
        setPreviewZoomStep: makeSetter("previewZoomStep"),
        setPreviewPosition: makeSetter("previewPosition"),
        setImageAutoHeight: makeSetter("imageAutoHeight"),
        setImageMaxHeight: makeSetter("imageMaxHeight"),
        setShowImageFileName: makeSetter("showImageFileName"),
        setColorTheme: makeSetter("colorTheme"),
        setSharpCorners: makeSetter("sharpCorners"),
        setAutoResetState: makeSetter("autoResetState"),
        setSearchAutoFocus: makeSetter("searchAutoFocus"),
        setSearchAutoClear: makeSetter("searchAutoClear"),
        setDarkMode: makeSetter("darkMode"),
        setCardDensity: makeSetter("cardDensity"),
        setTimeFormat: makeSetter("timeFormat"),
        setHoverPreviewDelay: makeSetter("hoverPreviewDelay"),
        setCopySound: makeSetter("copySound"),
        setCopySoundTiming: makeSetter("copySoundTiming"),
        setPasteSound: makeSetter("pasteSound"),
        setPasteSoundTiming: makeSetter("pasteSoundTiming"),
        setPasteCloseWindow: makeSetter("pasteCloseWindow"),
        setPasteMoveToTop: makeSetter("pasteMoveToTop"),
        setShowCategoryFilter: makeSetter("showCategoryFilter"),
        setShowDragAreaIndicator: makeSetter("showDragAreaIndicator"),
        setWindowAnimation: makeSetter("windowAnimation"),
        setToolbarButtons: makeSetter("toolbarButtons"),

        // Special setters with extra side effects
        setKeyboardNavigation: (enabled) => {
          set({ keyboardNavigation: enabled });
          broadcastChange({ keyboardNavigation: enabled });
          invoke("set_keyboard_nav_enabled", { enabled }).catch(() => {});
        },
        setWindowEffect: (effect) => {
          set({ windowEffect: effect });
          broadcastChange({ windowEffect: effect });
          document.documentElement.setAttribute("data-window-effect", effect);
          invoke("set_window_effect", { effect }).catch(() => {});
        },
      };
    },
    {
      name: STORAGE_KEY,
    }
  )
);

// Track listener to prevent duplicate registration
let unlistenFn: (() => void) | null = null;

// Initialize settings listener (called once per window)
export async function initUISettingsListener() {
  if (unlistenFn) return; // Already initialized
  
  try {
    unlistenFn = await listen<Partial<UISettings>>(SYNC_EVENT, (event) => {
      useUISettings.setState(event.payload);
    });
  } catch {
    // Ignore errors (e.g., in non-Tauri environment)
  }
}

// Cleanup listener (call on window close if needed)
export function cleanupUISettingsListener() {
  if (unlistenFn) {
    unlistenFn();
    unlistenFn = null;
  }
}

// Auto-initialize in browser environment
if (typeof window !== "undefined") {
  initUISettingsListener();
}
