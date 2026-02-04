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

// Listen for settings changes from other windows via Tauri events
if (typeof window !== "undefined") {
  listen<Partial<UISettings>>(SYNC_EVENT, (event) => {
    useUISettings.setState(event.payload);
  }).catch(() => {});
}
