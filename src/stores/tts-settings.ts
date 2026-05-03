import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";

const SYNC_EVENT = "tts-settings-changed";

const broadcastChange = (state: Partial<TtsSettings>) => {
  emit(SYNC_EVENT, state).catch((error) => {
    logError("Failed to broadcast TTS settings change:", error);
  });
};

export type TtsEngine = "edge" | "browser";

export interface TtsSettings {
  engine: TtsEngine;
  /** 统一模式声源 */
  edgeVoice: string;
  /** 分开模式 - 英文声源 */
  edgeVoiceEn: string;
  /** 分开模式 - 中文声源 */
  edgeVoiceZh: string;
  /** 统一模式语速 */
  edgeRate: string;
  /** 分开模式 - 英文语速 */
  edgeRateEn: string;
  /** 分开模式 - 中文语速 */
  edgeRateZh: string;
  splitVoice: boolean;
  proxyMode: "system" | "none" | "custom";
  proxyUrl: string;
  browserAccent: string;
  /** 朗读时逐词高亮 */
  highlightWord: boolean;
}

interface TtsSettingsStore extends TtsSettings {
  loaded: boolean;
  loadSettings: () => Promise<void>;
  saveSetting: (key: string, value: string) => Promise<void>;
  setEngine: (engine: TtsEngine) => void;
  setEdgeVoice: (voice: string) => void;
  setEdgeVoiceEn: (voice: string) => void;
  setEdgeVoiceZh: (voice: string) => void;
  setEdgeRate: (rate: string) => void;
  setEdgeRateEn: (rate: string) => void;
  setEdgeRateZh: (rate: string) => void;
  setSplitVoice: (split: boolean) => void;
  setProxyMode: (mode: "system" | "none" | "custom") => void;
  setProxyUrl: (url: string) => void;
  setBrowserAccent: (accent: string) => void;
  setHighlightWord: (highlight: boolean) => void;
}

const SETTING_KEYS: Record<string, keyof TtsSettings> = {
  tts_engine: "engine",
  tts_edge_voice: "edgeVoice",
  tts_edge_voice_en: "edgeVoiceEn",
  tts_edge_voice_zh: "edgeVoiceZh",
  tts_edge_rate: "edgeRate",
  tts_edge_rate_en: "edgeRateEn",
  tts_edge_rate_zh: "edgeRateZh",
  tts_split_voice: "splitVoice",
  tts_proxy_mode: "proxyMode",
  tts_proxy_url: "proxyUrl",
  tts_browser_accent: "browserAccent",
  tts_highlight_word: "highlightWord",
};

export const useTtsSettings = create<TtsSettingsStore>((set, get) => ({
  engine: "edge",
  edgeVoice: "zh-CN-XiaoxiaoNeural",
  edgeVoiceEn: "en-US-AriaNeural",
  edgeVoiceZh: "zh-CN-XiaoxiaoNeural",
  edgeRate: "+0%",
  edgeRateEn: "+0%",
  edgeRateZh: "+0%",
  splitVoice: false,
  proxyMode: "system",
  proxyUrl: "",
  browserAccent: "en-US",
  highlightWord: true,
  loaded: false,

  loadSettings: async () => {
    try {
      const keys = Object.keys(SETTING_KEYS);
      const values = await Promise.all(
        keys.map((key) => invoke<string | null>("get_setting", { key }))
      );
      const m = new Map(keys.map((k, i) => [k, values[i]]));
      set({
        engine: (m.get("tts_engine") as TtsEngine) || "edge",
        edgeVoice: m.get("tts_edge_voice") || "zh-CN-XiaoxiaoNeural",
        edgeVoiceEn: m.get("tts_edge_voice_en") || "en-US-AriaNeural",
        edgeVoiceZh: m.get("tts_edge_voice_zh") || "zh-CN-XiaoxiaoNeural",
        edgeRate: m.get("tts_edge_rate") || "+0%",
        edgeRateEn: m.get("tts_edge_rate_en") || "+0%",
        edgeRateZh: m.get("tts_edge_rate_zh") || "+0%",
        splitVoice: m.get("tts_split_voice") === "true",
        proxyMode: (m.get("tts_proxy_mode") as "system" | "none" | "custom") || "system",
        proxyUrl: m.get("tts_proxy_url") || "",
        browserAccent: m.get("tts_browser_accent") || "en-US",
        highlightWord: m.get("tts_highlight_word") !== "false",
        loaded: true,
      });
    } catch (error) {
      logError("加载 TTS 设置失败:", error);
    }
  },

  saveSetting: async (key: string, value: string) => {
    try {
      await invoke("set_setting", { key, value });
    } catch (error) {
      logError(`保存 ${key} 失败:`, error);
    }
  },

  setEngine: (engine) => {
    set({ engine });
    get().saveSetting("tts_engine", engine);
    broadcastChange({ engine });
  },
  setEdgeVoice: (edgeVoice) => {
    set({ edgeVoice });
    get().saveSetting("tts_edge_voice", edgeVoice);
    broadcastChange({ edgeVoice });
  },
  setEdgeVoiceEn: (edgeVoiceEn) => {
    set({ edgeVoiceEn });
    get().saveSetting("tts_edge_voice_en", edgeVoiceEn);
    broadcastChange({ edgeVoiceEn });
  },
  setEdgeVoiceZh: (edgeVoiceZh) => {
    set({ edgeVoiceZh });
    get().saveSetting("tts_edge_voice_zh", edgeVoiceZh);
    broadcastChange({ edgeVoiceZh });
  },
  setEdgeRate: (edgeRate) => {
    set({ edgeRate });
    get().saveSetting("tts_edge_rate", edgeRate);
    broadcastChange({ edgeRate });
  },
  setEdgeRateEn: (edgeRateEn) => {
    set({ edgeRateEn });
    get().saveSetting("tts_edge_rate_en", edgeRateEn);
    broadcastChange({ edgeRateEn });
  },
  setEdgeRateZh: (edgeRateZh) => {
    set({ edgeRateZh });
    get().saveSetting("tts_edge_rate_zh", edgeRateZh);
    broadcastChange({ edgeRateZh });
  },
  setSplitVoice: (splitVoice) => {
    set({ splitVoice });
    get().saveSetting("tts_split_voice", splitVoice ? "true" : "false");
    broadcastChange({ splitVoice });
  },
  setProxyMode: (proxyMode) => {
    set({ proxyMode });
    get().saveSetting("tts_proxy_mode", proxyMode);
    broadcastChange({ proxyMode });
  },
  setProxyUrl: (proxyUrl) => {
    set({ proxyUrl });
    get().saveSetting("tts_proxy_url", proxyUrl);
    broadcastChange({ proxyUrl });
  },
  setBrowserAccent: (browserAccent) => {
    set({ browserAccent });
    get().saveSetting("tts_browser_accent", browserAccent);
    broadcastChange({ browserAccent });
  },
  setHighlightWord: (highlightWord) => {
    set({ highlightWord });
    get().saveSetting("tts_highlight_word", highlightWord ? "true" : "false");
    broadcastChange({ highlightWord });
  },
}));

let unlistenFn: (() => void) | null = null;

export async function initTtsSettingsListener() {
  if (unlistenFn) return;
  try {
    unlistenFn = await listen<Partial<TtsSettings>>(SYNC_EVENT, (event) => {
      useTtsSettings.setState(event.payload);
    });
  } catch {
    // 非 Tauri 环境下忽略
  }
}

if (typeof window !== "undefined") {
  initTtsSettingsListener();
  useTtsSettings.getState().loadSettings();
}
