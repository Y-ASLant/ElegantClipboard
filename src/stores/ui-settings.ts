import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ColorTheme = "default" | "emerald" | "cyan" | "system";

interface UISettings {
  cardMaxLines: number;
  showTime: boolean;
  showCharCount: boolean;
  showByteSize: boolean;
  showSourceApp: boolean;
  sourceAppDisplay: "both" | "name" | "icon";
  imagePreviewEnabled: boolean;
  previewZoomStep: number;
  previewPosition: "auto" | "left" | "right";
  imageAutoHeight: boolean;
  imageMaxHeight: number;
  colorTheme: ColorTheme;
  sharpCorners: boolean;
  autoResetState: boolean;
  keyboardNavigation: boolean;
  searchAutoFocus: boolean;
  searchAutoClear: boolean;
  setCardMaxLines: (lines: number) => void;
  setShowTime: (show: boolean) => void;
  setShowCharCount: (show: boolean) => void;
  setShowByteSize: (show: boolean) => void;
  setShowSourceApp: (show: boolean) => void;
  setSourceAppDisplay: (mode: "both" | "name" | "icon") => void;
  setImagePreviewEnabled: (enabled: boolean) => void;
  setPreviewZoomStep: (step: number) => void;
  setPreviewPosition: (pos: "auto" | "left" | "right") => void;
  setImageAutoHeight: (auto: boolean) => void;
  setImageMaxHeight: (height: number) => void;
  setColorTheme: (theme: ColorTheme) => void;
  setSharpCorners: (enabled: boolean) => void;
  setAutoResetState: (enabled: boolean) => void;
  setKeyboardNavigation: (enabled: boolean) => void;
  setSearchAutoFocus: (enabled: boolean) => void;
  setSearchAutoClear: (enabled: boolean) => void;
}

const STORAGE_KEY = "clipboard-ui-settings";
const SYNC_EVENT = "ui-settings-changed";

// Helper to broadcast settings change
const broadcastChange = (state: Partial<UISettings>) => {
  emit(SYNC_EVENT, state).catch(() => {});
};

export const useUISettings = create<UISettings>()(
  persist(
    (set) => ({
      cardMaxLines: 3,
      showTime: true,
      showCharCount: true,
      showByteSize: true,
      showSourceApp: true,
      sourceAppDisplay: "both" as "both" | "name" | "icon",
      imagePreviewEnabled: false,
      previewZoomStep: 15,
      previewPosition: "auto" as "auto" | "left" | "right",
      imageAutoHeight: true,
      imageMaxHeight: 512,
      colorTheme: "system" as ColorTheme,
      sharpCorners: false,
      autoResetState: true,
      keyboardNavigation: true,
      searchAutoFocus: true,
      searchAutoClear: true,
      setCardMaxLines: (lines) => {
        set({ cardMaxLines: lines });
        broadcastChange({ cardMaxLines: lines });
      },
      setShowTime: (show) => {
        set({ showTime: show });
        broadcastChange({ showTime: show });
      },
      setShowCharCount: (show) => {
        set({ showCharCount: show });
        broadcastChange({ showCharCount: show });
      },
      setShowByteSize: (show) => {
        set({ showByteSize: show });
        broadcastChange({ showByteSize: show });
      },
      setShowSourceApp: (show) => {
        set({ showSourceApp: show });
        broadcastChange({ showSourceApp: show });
      },
      setSourceAppDisplay: (mode) => {
        set({ sourceAppDisplay: mode });
        broadcastChange({ sourceAppDisplay: mode });
      },
      setImagePreviewEnabled: (enabled) => {
        set({ imagePreviewEnabled: enabled });
        broadcastChange({ imagePreviewEnabled: enabled });
      },
      setPreviewZoomStep: (step) => {
        set({ previewZoomStep: step });
        broadcastChange({ previewZoomStep: step });
      },
      setPreviewPosition: (pos) => {
        set({ previewPosition: pos });
        broadcastChange({ previewPosition: pos });
      },
      setImageAutoHeight: (auto) => {
        set({ imageAutoHeight: auto });
        broadcastChange({ imageAutoHeight: auto });
      },
      setImageMaxHeight: (height) => {
        set({ imageMaxHeight: height });
        broadcastChange({ imageMaxHeight: height });
      },
      setColorTheme: (theme) => {
        set({ colorTheme: theme });
        broadcastChange({ colorTheme: theme });
      },
      setSharpCorners: (enabled) => {
        set({ sharpCorners: enabled });
        broadcastChange({ sharpCorners: enabled });
      },
      setAutoResetState: (enabled) => {
        set({ autoResetState: enabled });
        broadcastChange({ autoResetState: enabled });
      },
      setKeyboardNavigation: (enabled) => {
        set({ keyboardNavigation: enabled });
        broadcastChange({ keyboardNavigation: enabled });
      },
      setSearchAutoFocus: (enabled) => {
        set({ searchAutoFocus: enabled });
        broadcastChange({ searchAutoFocus: enabled });
      },
      setSearchAutoClear: (enabled) => {
        set({ searchAutoClear: enabled });
        broadcastChange({ searchAutoClear: enabled });
      },
    }),
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
