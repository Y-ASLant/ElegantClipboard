import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

const SYNC_EVENT = "ocr-settings-changed";

const broadcastChange = (state: Partial<OcrSettings>) => {
  emit(SYNC_EVENT, state).catch((error) => {
    logError("Failed to broadcast OCR settings change:", error);
  });
};

export type OcrProvider = "baidu";
export type OcrAccuracy = "high" | "standard";

export interface OcrSettings {
  enabled: boolean;
  recordOcrCopy: boolean;
  autoCopy: boolean;
  autoTranslate: boolean;
  provider: OcrProvider;
  accuracy: OcrAccuracy;
  shortcut: string;
  // Baidu OCR
  baiduApiKey: string;
  baiduSecretKey: string;
  // Proxy
  proxyMode: "system" | "none" | "custom";
  proxyUrl: string;
}

interface OcrSettingsStore extends OcrSettings {
  loaded: boolean;
  loadSettings: () => Promise<void>;
  saveSetting: (key: string, value: string) => Promise<void>;
  setEnabled: (enabled: boolean) => void;
  setRecordOcrCopy: (record: boolean) => void;
  setAutoCopy: (auto: boolean) => void;
  setAutoTranslate: (auto: boolean) => void;
  setProvider: (provider: OcrProvider) => void;
  setAccuracy: (accuracy: OcrAccuracy) => void;
  setShortcut: (shortcut: string) => void;
  setBaiduApiKey: (key: string) => void;
  setBaiduSecretKey: (key: string) => void;
  setProxyMode: (mode: "system" | "none" | "custom") => void;
  setProxyUrl: (url: string) => void;
}

const SETTING_KEYS: Record<string, keyof OcrSettings> = {
  ocr_enabled: "enabled",
  ocr_record_copy: "recordOcrCopy",
  ocr_auto_copy: "autoCopy",
  ocr_auto_translate: "autoTranslate",
  ocr_provider: "provider",
  ocr_accuracy: "accuracy",
  ocr_shortcut: "shortcut",
  ocr_baidu_api_key: "baiduApiKey",
  ocr_baidu_secret_key: "baiduSecretKey",
  ocr_proxy_mode: "proxyMode",
  ocr_proxy_url: "proxyUrl",
};

export const useOcrSettings = create<OcrSettingsStore>((set, get) => ({
  enabled: false,
  recordOcrCopy: false,
  autoCopy: false,
  autoTranslate: false,
  provider: "baidu",
  accuracy: "high",
  shortcut: "",
  baiduApiKey: "",
  baiduSecretKey: "",
  proxyMode: "none",
  proxyUrl: "",
  loaded: false,

  loadSettings: async () => {
    try {
      const keys = Object.keys(SETTING_KEYS);
      const values = await Promise.all(
        keys.map((key) => invoke<string | null>("get_setting", { key }))
      );
      const m = new Map(keys.map((k, i) => [k, values[i]]));
      set({
        enabled: m.get("ocr_enabled") === "true",
        recordOcrCopy: m.get("ocr_record_copy") === "true",
        autoCopy: m.get("ocr_auto_copy") === "true",
        autoTranslate: m.get("ocr_auto_translate") === "true",
        provider: (m.get("ocr_provider") as OcrProvider) || "baidu",
        accuracy: (m.get("ocr_accuracy") as OcrAccuracy) || "high",
        shortcut: m.get("ocr_shortcut") || "",
        baiduApiKey: m.get("ocr_baidu_api_key") || "",
        baiduSecretKey: m.get("ocr_baidu_secret_key") || "",
        proxyMode: (m.get("ocr_proxy_mode") as "system" | "none" | "custom") || "none",
        proxyUrl: m.get("ocr_proxy_url") || "",
        loaded: true,
      });
    } catch (error) {
      logError("加载 OCR 设置失败:", error);
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
    get().saveSetting("ocr_enabled", enabled ? "true" : "false");
    broadcastChange({ enabled });
  },
  setRecordOcrCopy: (record) => {
    set({ recordOcrCopy: record });
    get().saveSetting("ocr_record_copy", record ? "true" : "false");
    broadcastChange({ recordOcrCopy: record });
  },
  setAutoCopy: (auto) => {
    set({ autoCopy: auto });
    get().saveSetting("ocr_auto_copy", auto ? "true" : "false");
    broadcastChange({ autoCopy: auto });
  },
  setAutoTranslate: (auto) => {
    set({ autoTranslate: auto });
    get().saveSetting("ocr_auto_translate", auto ? "true" : "false");
    broadcastChange({ autoTranslate: auto });
  },
  setProvider: (provider) => {
    set({ provider });
    get().saveSetting("ocr_provider", provider);
    broadcastChange({ provider });
  },
  setAccuracy: (accuracy) => {
    set({ accuracy });
    get().saveSetting("ocr_accuracy", accuracy);
    broadcastChange({ accuracy });
  },
  setShortcut: (shortcut) => {
    set({ shortcut });
    get().saveSetting("ocr_shortcut", shortcut);
    broadcastChange({ shortcut });
  },
  setBaiduApiKey: (key) => {
    set({ baiduApiKey: key });
    get().saveSetting("ocr_baidu_api_key", key);
    broadcastChange({ baiduApiKey: key });
  },
  setBaiduSecretKey: (key) => {
    set({ baiduSecretKey: key });
    get().saveSetting("ocr_baidu_secret_key", key);
    broadcastChange({ baiduSecretKey: key });
  },
  setProxyMode: (mode) => {
    set({ proxyMode: mode });
    get().saveSetting("ocr_proxy_mode", mode);
    broadcastChange({ proxyMode: mode });
  },
  setProxyUrl: (url) => {
    set({ proxyUrl: url });
    get().saveSetting("ocr_proxy_url", url);
    broadcastChange({ proxyUrl: url });
  },
}));

let unlistenFn: (() => void) | null = null;

export async function initOcrSettingsListener() {
  if (unlistenFn) return;
  try {
    unlistenFn = await listen<Partial<OcrSettings>>(SYNC_EVENT, (event) => {
      useOcrSettings.setState(event.payload);
    });
  } catch {
    // 非 Tauri 环境下忽略
  }
}

if (typeof window !== "undefined") {
  initOcrSettingsListener();
}
