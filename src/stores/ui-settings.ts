import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { persist } from "zustand/middleware";

interface UISettings {
  cardMaxLines: number;
  showTime: boolean;
  showCharCount: boolean;
  showByteSize: boolean;
  imagePreviewEnabled: boolean;
  previewZoomStep: number;
  previewPosition: "auto" | "left" | "right";
  setCardMaxLines: (lines: number) => void;
  setShowTime: (show: boolean) => void;
  setShowCharCount: (show: boolean) => void;
  setShowByteSize: (show: boolean) => void;
  setImagePreviewEnabled: (enabled: boolean) => void;
  setPreviewZoomStep: (step: number) => void;
  setPreviewPosition: (pos: "auto" | "left" | "right") => void;
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
      imagePreviewEnabled: true,
      previewZoomStep: 15,
      previewPosition: "auto" as "auto" | "left" | "right",
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
