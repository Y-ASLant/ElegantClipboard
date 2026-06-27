import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

const SYNC_EVENT = "translate-settings-changed";

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

const DEFAULT_TRANSLATE_SETTINGS: TranslateSettings = {
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
};

const TRANSLATE_SETTINGS_KEYS = Object.keys(DEFAULT_TRANSLATE_SETTINGS) as (keyof TranslateSettings)[];

/**
 * Maps camelCase field names to their database key equivalents.
 * This is the single source of truth for the field-to-key mapping,
 * eliminating the previous pattern of scattering key strings across setters.
 */
const FIELD_TO_DB_KEY: Record<keyof TranslateSettings, string> = {
  enabled: "translate_enabled",
  recordTranslation: "translate_record_translation",
  provider: "translate_provider",
  languageMode: "translate_language_mode",
  sourceLanguage: "translate_source_language",
  targetLanguage: "translate_target_language",
  deeplxEndpoint: "translate_deeplx_endpoint",
  googleApiKey: "translate_google_api_key",
  baiduAppId: "translate_baidu_app_id",
  baiduSecretKey: "translate_baidu_secret_key",
  openaiEndpoint: "translate_openai_endpoint",
  openaiApiKey: "translate_openai_api_key",
  openaiModel: "translate_openai_model",
  proxyMode: "translate_proxy_mode",
  proxyUrl: "translate_proxy_url",
  translateSelectionEnabled: "translate_selection_enabled",
  translateSelectionShortcut: "translate_selection_shortcut",
};

const DB_KEYS = Object.values(FIELD_TO_DB_KEY);

function serializeValue(value: unknown): string {
  if (typeof value === "boolean") return value ? "true" : "false";
  return String(value);
}

function pickTranslateData(state: TranslateSettingsStore): TranslateSettings {
  const next = {} as TranslateSettings;
  for (const key of TRANSLATE_SETTINGS_KEYS) {
    (next[key] as TranslateSettings[typeof key]) = state[key];
  }
  return next;
}

function updateAndPersist(
  set: (partial: Partial<TranslateSettingsStore>) => void,
  get: () => TranslateSettingsStore,
  patch: Partial<TranslateSettings>,
) {
  set(patch as Partial<TranslateSettingsStore>);
  emit(SYNC_EVENT, patch).catch((error) => {
    logError("Failed to broadcast translate settings change:", error);
  });
  const snapshot = { ...pickTranslateData(get()), ...patch };
  saveTranslateSettings(snapshot).catch((error) => {
    logError("Failed to save translate settings:", error);
  });
}

async function saveTranslateSettings(state: TranslateSettings) {
  const entries = Object.entries(FIELD_TO_DB_KEY) as [keyof TranslateSettings, string][];
  const promises = entries.map(([field, dbKey]) =>
    invoke("set_setting", { key: dbKey, value: serializeValue(state[field]) }),
  );
  await Promise.all(promises);
}

export const useTranslateSettings = create<TranslateSettingsStore>((set, get) => {
  const makeSetter = <K extends keyof TranslateSettings>(key: K) =>
    (value: TranslateSettings[K]) => {
      updateAndPersist(set, get, { [key]: value } as Pick<TranslateSettings, K>);
    };

  return {
    ...DEFAULT_TRANSLATE_SETTINGS,
    loaded: false,

    loadSettings: async () => {
      try {
        const values = await invoke<Record<string, string>>("get_settings_batch", { keys: DB_KEYS });
        const dbKeyToField = Object.fromEntries(
          Object.entries(FIELD_TO_DB_KEY).map(([field, dbKey]) => [dbKey, field]),
        ) as Record<string, keyof TranslateSettings>;

        const parsed: Partial<TranslateSettings> = {};
        for (const [dbKey, value] of Object.entries(values)) {
          const field = dbKeyToField[dbKey];
          if (!field) continue;
          const def = DEFAULT_TRANSLATE_SETTINGS[field];
          if (typeof def === "boolean") {
            (parsed[field] as boolean) = value === "true";
          } else {
            (parsed[field] as string) = value || (def as string);
          }
        }
        set({ ...DEFAULT_TRANSLATE_SETTINGS, ...parsed, loaded: true });
      } catch (error) {
        logError("Failed to load translate settings:", error);
      }
    },

    setEnabled: makeSetter("enabled"),
    setRecordTranslation: makeSetter("recordTranslation"),
    setProvider: makeSetter("provider"),
    setLanguageMode: makeSetter("languageMode"),
    setSourceLanguage: makeSetter("sourceLanguage"),
    setTargetLanguage: makeSetter("targetLanguage"),
    setDeeplxEndpoint: makeSetter("deeplxEndpoint"),
    setGoogleApiKey: makeSetter("googleApiKey"),
    setBaiduAppId: makeSetter("baiduAppId"),
    setBaiduSecretKey: makeSetter("baiduSecretKey"),
    setOpenaiEndpoint: makeSetter("openaiEndpoint"),
    setOpenaiApiKey: makeSetter("openaiApiKey"),
    setOpenaiModel: makeSetter("openaiModel"),
    setProxyMode: makeSetter("proxyMode"),
    setProxyUrl: makeSetter("proxyUrl"),
    setTranslateSelectionEnabled: makeSetter("translateSelectionEnabled"),
    setTranslateSelectionShortcut: makeSetter("translateSelectionShortcut"),
  };
});

let unlistenFn: (() => void) | null = null;

export async function initTranslateSettingsListener() {
  if (unlistenFn) return;
  try {
    unlistenFn = await listen<Partial<TranslateSettings>>(SYNC_EVENT, (event) => {
      useTranslateSettings.setState(event.payload);
    });
  } catch {
    // non-Tauri environments
  }
}

if (typeof window !== "undefined") {
  initTranslateSettingsListener();
}
