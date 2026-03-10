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
  emit(SYNC_EVENT, state).catch(() => {});
};

export const useUISettings = create<UISettings>()(
  persist(
    (set) => {
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

// 浏览器环境自动初始化
if (typeof window !== "undefined") {
  initUISettingsListener();
}
