import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { logError } from "@/lib/logger";

export type ColorTheme = "default" | "emerald" | "cyan" | "system";
export type DarkMode = "light" | "dark" | "auto";
export type CardDensity = "compact" | "standard" | "spacious";
export type TimeFormat = "relative" | "absolute";
export type WindowEffect = "none" | "mica" | "acrylic" | "tabbed";
export type SoundTiming = "immediate" | "after_success";
export type ToolbarButton = "clear" | "pin" | "batch" | "settings";

export const DEFAULT_TOOLBAR_BUTTONS: ToolbarButton[] = ["clear", "batch", "pin", "settings"];
export const MAX_TOOLBAR_BUTTONS = 4;

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
  // 新增设置
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
  hideFavoritedFromMain: boolean;
  hideTaggedFromMain: boolean;
  enabledMonitorTypes: string[];
  toolbarButtons: ToolbarButton[];
  customFont: string;
  uiFontSize: number;
  cardFont: string;
  cardFontSize: number;
  previewFont: string;
  previewFontSize: number;
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
  setHideFavoritedFromMain: (enabled: boolean) => void;
  setHideTaggedFromMain: (enabled: boolean) => void;
  setEnabledMonitorTypes: (types: string[]) => void;
  setToolbarButtons: (buttons: ToolbarButton[]) => void;
  setCustomFont: (font: string) => void;
  setUIFontSize: (size: number) => void;
  setCardFont: (font: string) => void;
  setCardFontSize: (size: number) => void;
  setPreviewFont: (font: string) => void;
  setPreviewFontSize: (size: number) => void;
  resetFontSettings: () => void;
}

const STORAGE_KEY = "clipboard-ui-settings";
const SYNC_EVENT = "ui-settings-changed";

// 广播设置变更
const broadcastChange = (state: Partial<UISettings>) => {
  emit(SYNC_EVENT, state).catch((error) => {
    logError("Failed to broadcast UI settings change:", error);
  });
};

export const useUISettings = create<UISettings>()(
  persist(
    (set, get) => {
      // 工厂方法：创建更新状态并广播变更的 setter
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
        hideFavoritedFromMain: false,
        hideTaggedFromMain: false,
        enabledMonitorTypes: ["text", "image", "files", "video"],
        toolbarButtons: ["clear", "batch", "pin", "settings"] as ToolbarButton[],
        customFont: "",
        uiFontSize: 14,
        cardFont: "",
        cardFontSize: 14,
        previewFont: "",
        previewFontSize: 13,

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
        setHideFavoritedFromMain: (enabled) => {
          set({ hideFavoritedFromMain: enabled });
          broadcastChange({ hideFavoritedFromMain: enabled });
          invoke("set_setting", { key: "hide_favorited_from_main", value: String(enabled) }).catch((error) => {
            logError("Failed to persist hideFavoritedFromMain:", error);
          });
        },
        setHideTaggedFromMain: (enabled) => {
          set({ hideTaggedFromMain: enabled });
          broadcastChange({ hideTaggedFromMain: enabled });
          invoke("set_setting", { key: "hide_tagged_from_main", value: String(enabled) }).catch((error) => {
            logError("Failed to persist hideTaggedFromMain:", error);
          });
        },
        setEnabledMonitorTypes: (types) => {
          set({ enabledMonitorTypes: types });
          broadcastChange({ enabledMonitorTypes: types } as unknown as Partial<UISettings>);
        },
        setToolbarButtons: makeSetter("toolbarButtons"),
        setCustomFont: makeSetter("customFont"),
        setUIFontSize: makeSetter("uiFontSize"),
        setCardFont: makeSetter("cardFont"),
        setCardFontSize: makeSetter("cardFontSize"),
        setPreviewFont: makeSetter("previewFont"),
        setPreviewFontSize: makeSetter("previewFontSize"),
        resetFontSettings: () => {
          const defaults = { customFont: "", uiFontSize: 14, cardFont: "", cardFontSize: 14, previewFont: "", previewFontSize: 13 };
          set(defaults);
          broadcastChange(defaults);
        },

        // 带额外副作用的 setter
        setKeyboardNavigation: (enabled) => {
          const previous = get().keyboardNavigation;
          set({ keyboardNavigation: enabled });
          broadcastChange({ keyboardNavigation: enabled });
          invoke("set_keyboard_nav_enabled", { enabled }).catch((error) => {
            logError("Failed to set keyboard navigation:", error);
            set({ keyboardNavigation: previous });
            broadcastChange({ keyboardNavigation: previous });
          });
        },
        setWindowEffect: (effect) => {
          const previous = get().windowEffect;
          set({ windowEffect: effect });
          broadcastChange({ windowEffect: effect });
          document.documentElement.setAttribute("data-window-effect", effect);
          invoke("set_window_effect", { effect }).catch((error) => {
            logError("Failed to set window effect:", error);
            set({ windowEffect: previous });
            broadcastChange({ windowEffect: previous });
            document.documentElement.setAttribute("data-window-effect", previous);
          });
        },
      };
    },
    {
      name: STORAGE_KEY,
      version: 3,
      migrate: (persisted: unknown, version: number) => {
        const state = persisted as Record<string, unknown>;
        if (version < 1) {
          // v0 → v1: 工具栏新增 favorites 按钮
          const buttons = state.toolbarButtons as string[] | undefined;
          if (buttons && !buttons.includes("favorites")) {
            const idx = buttons.indexOf("clear");
            buttons.splice(idx >= 0 ? idx + 1 : 0, 0, "favorites");
            state.toolbarButtons = buttons;
          }
        }
        if (version < 2) {
          // v1 → v2: 工具栏新增 tags 按钮
          const buttons = state.toolbarButtons as string[] | undefined;
          if (buttons && !buttons.includes("tags")) {
            const idx = buttons.indexOf("favorites");
            buttons.splice(idx >= 0 ? idx + 1 : 0, 0, "tags");
            state.toolbarButtons = buttons;
          }
        }
        if (version < 3) {
          // v2 → v3: favorites 和 tags 改为常驻按钮，从工具栏移除
          const buttons = state.toolbarButtons as string[] | undefined;
          if (buttons) {
            state.toolbarButtons = buttons.filter(b => b !== "favorites" && b !== "tags");
          }
        }
        return state as unknown as UISettings;
      },
    }
  )
);

// 跟踪监听器防止重复注册
let unlistenFn: (() => void) | null = null;

// 初始化设置监听器（每个窗口调用一次）
export async function initUISettingsListener() {
  if (unlistenFn) return; // 已初始化
  
  try {
    unlistenFn = await listen<Partial<UISettings>>(SYNC_EVENT, (event) => {
      useUISettings.setState(event.payload);
    });
  } catch {
    // 忽略错误（如非 Tauri 环境）
  }
}

// 清理监听器
export function cleanupUISettingsListener() {
  if (unlistenFn) {
    unlistenFn();
    unlistenFn = null;
  }
}

// 从后端数据库加载需同步的设置（用于启动初始化和云端同步下载后刷新）
export async function loadSyncedSettings() {
  try {
    const keys = ["hide_favorited_from_main", "hide_tagged_from_main", "monitor_types"];
    const values = await Promise.all(
      keys.map((key) => invoke<string | null>("get_setting", { key }))
    );
    const patch: Record<string, unknown> = {};
    if (values[0] !== null && values[0] !== undefined) patch.hideFavoritedFromMain = values[0] === "true";
    if (values[1] !== null && values[1] !== undefined) patch.hideTaggedFromMain = values[1] === "true";
    if (values[2] && (values[2] as string).length > 0) {
      const rawSet = new Set((values[2] as string).split(",").map((t) => t.trim()).filter(Boolean));
      const uiTypes: string[] = [];
      if (rawSet.has("text") || rawSet.has("html") || rawSet.has("rtf")) uiTypes.push("text");
      if (rawSet.has("image")) uiTypes.push("image");
      if (rawSet.has("files")) uiTypes.push("files");
      if (rawSet.has("video")) uiTypes.push("video");
      if (uiTypes.length > 0) patch.enabledMonitorTypes = uiTypes;
    }
    if (Object.keys(patch).length > 0) {
      useUISettings.setState(patch);
      broadcastChange(patch as Partial<UISettings>);
    }
  } catch {
    // 忽略错误（如非 Tauri 环境）
  }
}

// 浏览器环境自动初始化
if (typeof window !== "undefined") {
  initUISettingsListener();
  loadSyncedSettings();
}
