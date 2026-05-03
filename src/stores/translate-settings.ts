import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

const SYNC_EVENT = "translate-settings-changed";

// 广播设置变更到其他窗口
const broadcastChange = (state: Partial<TranslateSettings>) => {
  emit(SYNC_EVENT, state).catch((error) => {
    logError("Failed to broadcast translate settings change:", error);
  });
};

export type TranslateProvider = "microsoft" | "google_free" | "google_api" | "baidu" | "deeplx" | "openai";
export type LanguageMode = "auto" | "manual";

export interface TranslateSettings {
  enabled: boolean;
  recordTranslation: boolean;
  provider: TranslateProvider;
  languageMode: LanguageMode;
  sourceLanguage: string;
  targetLanguage: string;
  // DeepLX
  deeplxEndpoint: string;
  // Google API
  googleApiKey: string;
  // Baidu
  baiduAppId: string;
  baiduSecretKey: string;
  // OpenAI
  openaiEndpoint: string;
  openaiApiKey: string;
  openaiModel: string;
  // Proxy
  proxyMode: "system" | "none" | "custom";
  proxyUrl: string;
  // Translate selection
  translateSelectionEnabled: boolean;
  translateSelectionShortcut: string;
}

interface TranslateSettingsStore extends TranslateSettings {
  loaded: boolean;
  loadSettings: () => Promise<void>;
  saveSetting: (key: string, value: string) => Promise<void>;
  setEnabled: (enabled: boolean) => void;
  setRecordTranslation: (record: boolean) => void;
  setProvider: (provider: TranslateProvider) => void;
  setLanguageMode: (mode: LanguageMode) => void;
  setSourceLanguage: (lang: string) => void;
  setTargetLanguage: (lang: string) => void;
  setDeeplxEndpoint: (url: string) => void;
  setGoogleApiKey: (key: string) => void;
  setBaiduAppId: (id: string) => void;
  setBaiduSecretKey: (key: string) => void;
  setOpenaiEndpoint: (url: string) => void;
  setOpenaiApiKey: (key: string) => void;
  setOpenaiModel: (model: string) => void;
  setProxyMode: (mode: "system" | "none" | "custom") => void;
  setProxyUrl: (url: string) => void;
  setTranslateSelectionEnabled: (enabled: boolean) => void;
  setTranslateSelectionShortcut: (shortcut: string) => void;
}

const SETTING_KEYS: Record<string, keyof TranslateSettings> = {
  translate_enabled: "enabled",
  translate_record_translation: "recordTranslation",
  translate_provider: "provider",
  translate_language_mode: "languageMode",
  translate_source_language: "sourceLanguage",
  translate_target_language: "targetLanguage",
  translate_deeplx_endpoint: "deeplxEndpoint",
  translate_google_api_key: "googleApiKey",
  translate_baidu_app_id: "baiduAppId",
  translate_baidu_secret_key: "baiduSecretKey",
  translate_openai_endpoint: "openaiEndpoint",
  translate_openai_api_key: "openaiApiKey",
  translate_openai_model: "openaiModel",
  translate_proxy_mode: "proxyMode",
  translate_proxy_url: "proxyUrl",
  translate_selection_enabled: "translateSelectionEnabled",
  translate_selection_shortcut: "translateSelectionShortcut",
};

export const useTranslateSettings = create<TranslateSettingsStore>((set, get) => ({
  enabled: false,
  recordTranslation: false,
  provider: "microsoft",
  languageMode: "auto",
  sourceLanguage: "",
  targetLanguage: "",
  deeplxEndpoint: "",
  googleApiKey: "",
  baiduAppId: "",
  baiduSecretKey: "",
  openaiEndpoint: "",
  openaiApiKey: "",
  openaiModel: "",
  proxyMode: "system",
  proxyUrl: "",
  translateSelectionEnabled: false,
  translateSelectionShortcut: "",
  loaded: false,

  loadSettings: async () => {
    try {
      const keys = Object.keys(SETTING_KEYS);
      const values = await Promise.all(
        keys.map((key) => invoke<string | null>("get_setting", { key }))
      );
      const m = new Map(keys.map((k, i) => [k, values[i]]));
      set({
        enabled: m.get("translate_enabled") === "true",
        recordTranslation: m.get("translate_record_translation") === "true",
        provider: (m.get("translate_provider") as TranslateProvider) || "microsoft",
        languageMode: (m.get("translate_language_mode") as LanguageMode) || "auto",
        sourceLanguage: m.get("translate_source_language") || "",
        targetLanguage: m.get("translate_target_language") || "",
        deeplxEndpoint: m.get("translate_deeplx_endpoint") || "",
        googleApiKey: m.get("translate_google_api_key") || "",
        baiduAppId: m.get("translate_baidu_app_id") || "",
        baiduSecretKey: m.get("translate_baidu_secret_key") || "",
        openaiEndpoint: m.get("translate_openai_endpoint") || "",
        openaiApiKey: m.get("translate_openai_api_key") || "",
        openaiModel: m.get("translate_openai_model") || "",
        proxyMode: (m.get("translate_proxy_mode") as "system" | "none" | "custom") || "system",
        proxyUrl: m.get("translate_proxy_url") || "",
        translateSelectionEnabled: m.get("translate_selection_enabled") === "true",
        translateSelectionShortcut: m.get("translate_selection_shortcut") || "",
        loaded: true,
      });
    } catch (error) {
      logError("加载翻译设置失败:", error);
    }
  },

  saveSetting: async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (error) {
      logError(`保存 ${key} 失败:`, error);
    }
  },

  setEnabled: (enabled) => {
    set({ enabled });
    get().saveSetting("translate_enabled", enabled ? "true" : "false");
    broadcastChange({ enabled });
  },
  setRecordTranslation: (record) => {
    set({ recordTranslation: record });
    get().saveSetting("translate_record_translation", record ? "true" : "false");
    broadcastChange({ recordTranslation: record });
  },
  setProvider: (provider) => {
    set({ provider });
    get().saveSetting("translate_provider", provider);
    broadcastChange({ provider });
  },
  setLanguageMode: (mode) => {
    set({ languageMode: mode });
    get().saveSetting("translate_language_mode", mode);
    broadcastChange({ languageMode: mode });
  },
  setSourceLanguage: (lang) => {
    set({ sourceLanguage: lang });
    get().saveSetting("translate_source_language", lang);
    broadcastChange({ sourceLanguage: lang });
  },
  setTargetLanguage: (lang) => {
    set({ targetLanguage: lang });
    get().saveSetting("translate_target_language", lang);
    broadcastChange({ targetLanguage: lang });
  },
  setDeeplxEndpoint: (url) => {
    set({ deeplxEndpoint: url });
    get().saveSetting("translate_deeplx_endpoint", url);
    broadcastChange({ deeplxEndpoint: url });
  },
  setGoogleApiKey: (key) => {
    set({ googleApiKey: key });
    get().saveSetting("translate_google_api_key", key);
    broadcastChange({ googleApiKey: key });
  },
  setBaiduAppId: (id) => {
    set({ baiduAppId: id });
    get().saveSetting("translate_baidu_app_id", id);
    broadcastChange({ baiduAppId: id });
  },
  setBaiduSecretKey: (key) => {
    set({ baiduSecretKey: key });
    get().saveSetting("translate_baidu_secret_key", key);
    broadcastChange({ baiduSecretKey: key });
  },
  setOpenaiEndpoint: (url) => {
    set({ openaiEndpoint: url });
    get().saveSetting("translate_openai_endpoint", url);
    broadcastChange({ openaiEndpoint: url });
  },
  setOpenaiApiKey: (key) => {
    set({ openaiApiKey: key });
    get().saveSetting("translate_openai_api_key", key);
    broadcastChange({ openaiApiKey: key });
  },
  setOpenaiModel: (model) => {
    set({ openaiModel: model });
    get().saveSetting("translate_openai_model", model);
    broadcastChange({ openaiModel: model });
  },
  setProxyMode: (mode) => {
    set({ proxyMode: mode });
    get().saveSetting("translate_proxy_mode", mode);
    broadcastChange({ proxyMode: mode });
  },
  setProxyUrl: (url) => {
    set({ proxyUrl: url });
    get().saveSetting("translate_proxy_url", url);
    broadcastChange({ proxyUrl: url });
  },
  setTranslateSelectionEnabled: (enabled) => {
    set({ translateSelectionEnabled: enabled });
    get().saveSetting("translate_selection_enabled", enabled ? "true" : "false");
    broadcastChange({ translateSelectionEnabled: enabled });
  },
  setTranslateSelectionShortcut: (shortcut) => {
    set({ translateSelectionShortcut: shortcut });
    get().saveSetting("translate_selection_shortcut", shortcut);
    broadcastChange({ translateSelectionShortcut: shortcut });
  },
}));

// 跟踪监听器防止重复注册
let unlistenFn: (() => void) | null = null;

// 初始化跨窗口设置监听器（每个窗口调用一次）
export async function initTranslateSettingsListener() {
  if (unlistenFn) return;
  try {
    unlistenFn = await listen<Partial<TranslateSettings>>(SYNC_EVENT, (event) => {
      useTranslateSettings.setState(event.payload);
    });
  } catch {
    // 非 Tauri 环境下忽略
  }
}

// 浏览器环境自动初始化
if (typeof window !== "undefined") {
  initTranslateSettingsListener();
}
