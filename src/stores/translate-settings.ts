import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

const SYNC_EVENT = "translate-settings-changed";

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
  deeplxEndpoint: string;
  googleApiKey: string;
  baiduAppId: string;
  baiduSecretKey: string;
  openaiEndpoint: string;
  openaiApiKey: string;
  openaiModel: string;
  proxyMode: "system" | "none" | "custom";
  proxyUrl: string;
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

const SETTING_KEYS = [
  "translate_enabled", "translate_record_translation", "translate_provider",
  "translate_language_mode", "translate_source_language", "translate_target_language",
  "translate_deeplx_endpoint", "translate_google_api_key",
  "translate_baidu_app_id", "translate_baidu_secret_key",
  "translate_openai_endpoint", "translate_openai_api_key", "translate_openai_model",
  "translate_proxy_mode", "translate_proxy_url",
  "translate_selection_enabled", "translate_selection_shortcut",
] as const;

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
      const values = await invoke<Record<string, string>>("get_settings_batch", { keys: SETTING_KEYS });
      set({
        enabled: values["translate_enabled"] === "true",
        recordTranslation: values["translate_record_translation"] === "true",
        provider: (values["translate_provider"] as TranslateProvider) || "microsoft",
        languageMode: (values["translate_language_mode"] as LanguageMode) || "auto",
        sourceLanguage: values["translate_source_language"] || "",
        targetLanguage: values["translate_target_language"] || "",
        deeplxEndpoint: values["translate_deeplx_endpoint"] || "",
        googleApiKey: values["translate_google_api_key"] || "",
        baiduAppId: values["translate_baidu_app_id"] || "",
        baiduSecretKey: values["translate_baidu_secret_key"] || "",
        openaiEndpoint: values["translate_openai_endpoint"] || "",
        openaiApiKey: values["translate_openai_api_key"] || "",
        openaiModel: values["translate_openai_model"] || "",
        proxyMode: (values["translate_proxy_mode"] as "system" | "none" | "custom") || "system",
        proxyUrl: values["translate_proxy_url"] || "",
        translateSelectionEnabled: values["translate_selection_enabled"] === "true",
        translateSelectionShortcut: values["translate_selection_shortcut"] || "",
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

  setEnabled: (enabled) => { set({ enabled }); get().saveSetting("translate_enabled", enabled ? "true" : "false"); broadcastChange({ enabled }); },
  setRecordTranslation: (record) => { set({ recordTranslation: record }); get().saveSetting("translate_record_translation", record ? "true" : "false"); broadcastChange({ recordTranslation: record }); },
  setProvider: (provider) => { set({ provider }); get().saveSetting("translate_provider", provider); broadcastChange({ provider }); },
  setLanguageMode: (mode) => { set({ languageMode: mode }); get().saveSetting("translate_language_mode", mode); broadcastChange({ languageMode: mode }); },
  setSourceLanguage: (lang) => { set({ sourceLanguage: lang }); get().saveSetting("translate_source_language", lang); broadcastChange({ sourceLanguage: lang }); },
  setTargetLanguage: (lang) => { set({ targetLanguage: lang }); get().saveSetting("translate_target_language", lang); broadcastChange({ targetLanguage: lang }); },
  setDeeplxEndpoint: (url) => { set({ deeplxEndpoint: url }); get().saveSetting("translate_deeplx_endpoint", url); broadcastChange({ deeplxEndpoint: url }); },
  setGoogleApiKey: (key) => { set({ googleApiKey: key }); get().saveSetting("translate_google_api_key", key); broadcastChange({ googleApiKey: key }); },
  setBaiduAppId: (id) => { set({ baiduAppId: id }); get().saveSetting("translate_baidu_app_id", id); broadcastChange({ baiduAppId: id }); },
  setBaiduSecretKey: (key) => { set({ baiduSecretKey: key }); get().saveSetting("translate_baidu_secret_key", key); broadcastChange({ baiduSecretKey: key }); },
  setOpenaiEndpoint: (url) => { set({ openaiEndpoint: url }); get().saveSetting("translate_openai_endpoint", url); broadcastChange({ openaiEndpoint: url }); },
  setOpenaiApiKey: (key) => { set({ openaiApiKey: key }); get().saveSetting("translate_openai_api_key", key); broadcastChange({ openaiApiKey: key }); },
  setOpenaiModel: (model) => { set({ openaiModel: model }); get().saveSetting("translate_openai_model", model); broadcastChange({ openaiModel: model }); },
  setProxyMode: (mode) => { set({ proxyMode: mode }); get().saveSetting("translate_proxy_mode", mode); broadcastChange({ proxyMode: mode }); },
  setProxyUrl: (url) => { set({ proxyUrl: url }); get().saveSetting("translate_proxy_url", url); broadcastChange({ proxyUrl: url }); },
  setTranslateSelectionEnabled: (enabled) => { set({ translateSelectionEnabled: enabled }); get().saveSetting("translate_selection_enabled", enabled ? "true" : "false"); broadcastChange({ translateSelectionEnabled: enabled }); },
  setTranslateSelectionShortcut: (shortcut) => { set({ translateSelectionShortcut: shortcut }); get().saveSetting("translate_selection_shortcut", shortcut); broadcastChange({ translateSelectionShortcut: shortcut }); },
}));

let unlistenFn: (() => void) | null = null;

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

if (typeof window !== "undefined") {
  initTranslateSettingsListener();
}
