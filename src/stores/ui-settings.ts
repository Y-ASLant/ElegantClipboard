import { create } from "zustand";
import { persist } from "zustand/middleware";
import { emit, listen } from "@tauri-apps/api/event";

interface UISettings {
  cardMaxLines: number;
  showTime: boolean;
  showCharCount: boolean;
  showByteSize: boolean;
  setCardMaxLines: (lines: number) => void;
  setShowTime: (show: boolean) => void;
  setShowCharCount: (show: boolean) => void;
  setShowByteSize: (show: boolean) => void;
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
