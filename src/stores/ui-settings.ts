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
  videoPreviewEnabled: boolean;
  videoPreviewDuration: number;
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
  setVideoPreviewEnabled: (enabled: boolean) => void;
  setVideoPreviewDuration: (duration: number) => void;
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

// camelCase → snake_case 转换
const toSnakeCase = (s: string) => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

// 需要同步到数据库的 makeSetter key（排除有独立 setter 的项）
const SYNCED_UI_KEYS: (keyof UISettings)[] = [
  "cardMaxLines", "showTime", "showCharCount", "showByteSize",
  "showSourceApp", "sourceAppDisplay",
  "imagePreviewEnabled", "textPreviewEnabled",
  "videoPreviewEnabled", "videoPreviewDuration",
  "previewUnboundedMode", "previewZoomStep", "previewPosition",
  "imageAutoHeight", "imageMaxHeight", "showImageFileName",
  "colorTheme", "sharpCorners", "autoResetState",
  "searchAutoFocus", "searchAutoClear",
  "darkMode", "cardDensity", "timeFormat", "hoverPreviewDelay",
  "pasteCloseWindow", "pasteMoveToTop",
  "showCategoryFilter", "showDragAreaIndicator", "windowAnimation",
  "toolbarButtons",
  "customFont", "uiFontSize", "cardFont", "cardFontSize",
  "previewFont", "previewFontSize",
];

const syncedKeySet = new Set<string>(SYNCED_UI_KEYS);

export const useUISettings = create<UISettings>()(
  persist(
    (set, get) => {
      // 工厂方法：创建更新状态并广播变更的 setter
      const makeSetter = <K extends keyof UISettings>(key: K) =>
        (value: UISettings[K]) => {
          set({ [key]: value } as unknown as Partial<UISettings>);
          broadcastChange({ [key]: value } as unknown as Partial<UISettings>);
          // 同步到数据库（供云端备份）
          if (syncedKeySet.has(key)) {
            const dbKey = `ui_${toSnakeCase(key)}`;
            const dbVal = typeof value === "object" ? JSON.stringify(value) : String(value);
            invoke("set_setting", { key: dbKey, value: dbVal }).catch((e) =>
              logError(`Failed to persist ${dbKey}:`, e),
            );
          }
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
        videoPreviewEnabled: false,
        videoPreviewDuration: 5,
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
        setVideoPreviewEnabled: makeSetter("videoPreviewEnabled"),
        setVideoPreviewDuration: makeSetter("videoPreviewDuration"),
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

// 浅比较：仅当 payload 中的值实际变化时才更新 store
function shallowPatchState(payload: Partial<UISettings>) {
  const current = useUISettings.getState();
  const patch: Record<string, unknown> = {};
  let hasChange = false;
  for (const [key, value] of Object.entries(payload)) {
    const existing = (current as unknown as Record<string, unknown>)[key];
    // 数组和对象使用 JSON 比较，原始值直接比较
    if (Array.isArray(value) || (typeof value === "object" && value !== null)) {
      if (JSON.stringify(existing) !== JSON.stringify(value)) {
        patch[key] = value;
        hasChange = true;
      }
    } else if (existing !== value) {
      patch[key] = value;
      hasChange = true;
    }
  }
  if (hasChange) {
    useUISettings.setState(patch);
  }
}

// 初始化设置监听器（每个窗口调用一次）
export async function initUISettingsListener() {
  if (unlistenFn) return; // 已初始化
  
  try {
    unlistenFn = await listen<Partial<UISettings>>(SYNC_EVENT, (event) => {
      shallowPatchState(event.payload);
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
    // 使用批量命令一次性获取所有需要的设置，减少 IPC 往返
    const legacyKeys = ["hide_favorited_from_main", "hide_tagged_from_main", "monitor_types"];
    const dbKeys = SYNCED_UI_KEYS.map((k) => `ui_${toSnakeCase(k)}`);
    const allKeys = [...legacyKeys, ...dbKeys];

    const allValues = await invoke<Record<string, string>>("get_settings_batch", { keys: allKeys });

    const patch: Record<string, unknown> = {};

    // 1. 处理原有的独立设置
    const hideFav = allValues["hide_favorited_from_main"];
    if (hideFav !== undefined) patch.hideFavoritedFromMain = hideFav === "true";
    const hideTag = allValues["hide_tagged_from_main"];
    if (hideTag !== undefined) patch.hideTaggedFromMain = hideTag === "true";
    const monitorTypes = allValues["monitor_types"];
    if (monitorTypes && monitorTypes.length > 0) {
      const rawSet = new Set(monitorTypes.split(",").map((t) => t.trim()).filter(Boolean));
      const uiTypes: string[] = [];
      if (rawSet.has("text") || rawSet.has("html") || rawSet.has("rtf")) uiTypes.push("text");
      if (rawSet.has("image")) uiTypes.push("image");
      if (rawSet.has("files")) uiTypes.push("files");
      if (rawSet.has("video")) uiTypes.push("video");
      if (uiTypes.length > 0) patch.enabledMonitorTypes = uiTypes;
    }

    // 2. 处理 makeSetter 同步的 UI 设置
    const defaults = useUISettings.getState();
    for (let i = 0; i < SYNCED_UI_KEYS.length; i++) {
      const raw = allValues[dbKeys[i]];
      if (raw === undefined) continue;
      const uiKey = SYNCED_UI_KEYS[i];
      const defVal = defaults[uiKey];
      // 按默认值类型还原
      if (typeof defVal === "boolean") {
        patch[uiKey] = raw === "true";
      } else if (typeof defVal === "number") {
        const n = Number(raw);
        if (!Number.isNaN(n)) patch[uiKey] = n;
      } else if (Array.isArray(defVal)) {
        try { patch[uiKey] = JSON.parse(raw); } catch { /* skip */ }
      } else {
        patch[uiKey] = raw;
      }
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
