import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

export type ColorTheme = "default" | "emerald" | "cyan" | "system";
export type DarkMode = "light" | "dark" | "auto";
export type CardDensity = "compact" | "standard" | "spacious";
export type TimeFormat = "relative" | "absolute";
export type WindowEffect = "none" | "mica" | "acrylic" | "tabbed";
export type SoundTiming = "immediate" | "after_success";
export type ToolbarButton = "clear" | "pin" | "batch" | "settings";

export const DEFAULT_TOOLBAR_BUTTONS: ToolbarButton[] = ["clear", "batch", "pin", "settings"];
export const MAX_TOOLBAR_BUTTONS = 5;

const UI_SETTINGS_DB_KEY = "ui_settings_json";
const LEGACY_UI_SETTINGS_STORAGE_KEY = "clipboard-ui-settings";
const SYNC_EVENT = "ui-settings-changed";

interface UISettingsData {
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
  customFont: string;
  uiFontSize: number;
  cardFont: string;
  cardFontSize: number;
  previewFont: string;
  previewFontSize: number;
  onboardingCompleted: boolean;
}

const DEFAULT_UI_SETTINGS: UISettingsData = {
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
  toolbarButtons: DEFAULT_TOOLBAR_BUTTONS,
  customFont: "",
  uiFontSize: 14,
  cardFont: "",
  cardFontSize: 14,
  previewFont: "",
  previewFontSize: 13,
  onboardingCompleted: false,
};

interface UISettings extends UISettingsData {
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
  setCustomFont: (font: string) => void;
  setUIFontSize: (size: number) => void;
  setCardFont: (font: string) => void;
  setCardFontSize: (size: number) => void;
  setPreviewFont: (font: string) => void;
  setPreviewFontSize: (size: number) => void;
  setOnboardingCompleted: (completed: boolean) => void;
  resetFontSettings: () => void;
}

const UI_SETTINGS_KEYS = Object.keys(DEFAULT_UI_SETTINGS) as (keyof UISettingsData)[];

function serializeUISettings(state: UISettingsData): string {
  return JSON.stringify(state);
}

function pickUISettingsData(state: UISettings): UISettingsData {
  const next = {} as UISettingsData;
  for (const key of UI_SETTINGS_KEYS) {
    (next[key] as UISettingsData[typeof key]) = state[key];
  }
  return next;
}

function mergeUISettings(raw: unknown): UISettingsData {
  if (!raw || typeof raw !== "object") {
    return { ...DEFAULT_UI_SETTINGS };
  }

  const persisted = raw as Partial<UISettingsData>;
  return {
    ...DEFAULT_UI_SETTINGS,
    ...persisted,
    toolbarButtons: Array.isArray(persisted.toolbarButtons) && persisted.toolbarButtons.length > 0
      ? persisted.toolbarButtons.filter((button): button is ToolbarButton =>
        ["clear", "pin", "batch", "settings"].includes(button),
      )
      : DEFAULT_TOOLBAR_BUTTONS,
  };
}

function readLegacyUISettings(): UISettingsData | null {
  if (typeof window === "undefined") {
    return null;
  }

  const raw = window.localStorage.getItem(LEGACY_UI_SETTINGS_STORAGE_KEY);
  if (!raw) {
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as { state?: unknown } | unknown;
    const state = parsed && typeof parsed === "object" && "state" in parsed
      ? (parsed as { state?: unknown }).state
      : parsed;
    return mergeUISettings(state);
  } catch (error) {
    logError("Failed to parse legacy UI settings:", error);
    return null;
  }
}

function clearLegacyUISettings() {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.removeItem(LEGACY_UI_SETTINGS_STORAGE_KEY);
  } catch (error) {
    logError("Failed to clear legacy UI settings:", error);
  }
}

async function saveUISettings(state: UISettingsData) {
  await invoke("set_setting", {
    key: UI_SETTINGS_DB_KEY,
    value: serializeUISettings(state),
  });
}

// 广播设置变更
const broadcastChange = (state: Partial<UISettingsData>) => {
  emit(SYNC_EVENT, state).catch((error) => {
    logError("Failed to broadcast UI settings change:", error);
  });
};

function updateAndPersist(
  set: (partial: Partial<UISettings>) => void,
  get: () => UISettings,
  patch: Partial<UISettingsData>,
) {
  set(patch as Partial<UISettings>);
  broadcastChange(patch);
  const snapshot = { ...pickUISettingsData(get()), ...patch };
  saveUISettings(snapshot).catch((error) => {
    logError("Failed to save UI settings:", error);
  });
}

export const useUISettings = create<UISettings>()((set, get) => {
  const makeSetter = <K extends keyof UISettingsData>(key: K) =>
    (value: UISettingsData[K]) => {
      updateAndPersist(set, get, { [key]: value } as Pick<UISettingsData, K>);
    };

  return {
    ...DEFAULT_UI_SETTINGS,

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
    setCustomFont: makeSetter("customFont"),
    setUIFontSize: makeSetter("uiFontSize"),
    setCardFont: makeSetter("cardFont"),
    setCardFontSize: makeSetter("cardFontSize"),
    setPreviewFont: makeSetter("previewFont"),
    setPreviewFontSize: makeSetter("previewFontSize"),
    setOnboardingCompleted: makeSetter("onboardingCompleted"),
    resetFontSettings: () => {
      const defaults = {
        customFont: "",
        uiFontSize: 14,
        cardFont: "",
        cardFontSize: 14,
        previewFont: "",
        previewFontSize: 13,
      };
      updateAndPersist(set, get, defaults);
    },

    setKeyboardNavigation: (enabled) => {
      const previous = get().keyboardNavigation;
      updateAndPersist(set, get, { keyboardNavigation: enabled });
      invoke("set_keyboard_nav_enabled", { enabled }).catch((error) => {
        logError("Failed to set keyboard navigation:", error);
        updateAndPersist(set, get, { keyboardNavigation: previous });
      });
    },
    setWindowEffect: (effect) => {
      const previous = get().windowEffect;
      set({ windowEffect: effect });
      broadcastChange({ windowEffect: effect });
      document.documentElement.setAttribute("data-window-effect", effect);
      saveUISettings({ ...pickUISettingsData(get()), windowEffect: effect }).catch((error) => {
        logError("Failed to save window effect:", error);
      });
      invoke("set_window_effect", { effect }).catch((error) => {
        logError("Failed to set window effect:", error);
        updateAndPersist(set, get, { windowEffect: previous });
        document.documentElement.setAttribute("data-window-effect", previous);
      });
    },
  };
});

let initPromise: Promise<void> | null = null;
let initialized = false;
let unlistenFn: (() => void) | null = null;

export function loadUISettingsFromBackend() {
  if (typeof window === "undefined") {
    return Promise.resolve();
  }

  return invoke<string | null>("get_setting", { key: UI_SETTINGS_DB_KEY })
    .then(async (value) => {
      if (value) {
        try {
          const parsed = JSON.parse(value);
          useUISettings.setState(mergeUISettings(parsed));
          clearLegacyUISettings();
        } catch (error) {
          logError("Failed to parse UI settings:", error);
        }
        return;
      }

      const legacy = readLegacyUISettings();
      if (!legacy) {
        return;
      }

      useUISettings.setState(legacy);
      await saveUISettings(legacy);
      clearLegacyUISettings();
    })
    .catch((error) => {
      logError("Failed to load UI settings:", error);
    });
}

// 初始化设置监听器和后端配置加载（每个窗口调用一次）
export function initUISettingsStore() {
  if (initPromise) {
    return initPromise;
  }

  initPromise = (async () => {
    if (!unlistenFn) {
      try {
        unlistenFn = await listen<Partial<UISettingsData>>(SYNC_EVENT, (event) => {
          useUISettings.setState(event.payload);
        });
      } catch {
        // ignore in non-tauri environments
      }
    }

    await loadUISettingsFromBackend();
    initialized = true;
  })();

  return initPromise;
}

export function isUISettingsInitialized() {
  return initialized;
}

export function whenUISettingsReady() {
  return initUISettingsStore();
}

export function cleanupUISettingsListener() {
  if (unlistenFn) {
    unlistenFn();
    unlistenFn = null;
  }
  initPromise = null;
  initialized = false;
}
