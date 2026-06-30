import { useMemo, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { logError } from "@/lib/logger";
import {
  applyDocumentLocale,
  createTranslator,
  DEFAULT_LOCALE,
  normalizeLocale,
} from "./core/translator";
import type { Locale } from "./types";

const SYNC_EVENT = "locale-changed";
const DB_KEY = "language";

interface LocaleState {
  locale: Locale;
  loaded: boolean;
  init: () => Promise<void>;
  setLocale: (locale: Locale) => Promise<void>;
}

export const useLocaleStore = create<LocaleState>((set, get) => ({
  locale: DEFAULT_LOCALE,
  loaded: false,

  init: async () => {
    try {
      const raw = await invoke<string | null>("get_setting", { key: DB_KEY });
      const locale = normalizeLocale(raw);
      applyDocumentLocale(locale);
      set({ locale, loaded: true });
    } catch {
      applyDocumentLocale(DEFAULT_LOCALE);
      set({ locale: DEFAULT_LOCALE, loaded: true });
    }
  },

  setLocale: async (locale) => {
    const previous = get().locale;
    if (locale === previous) return;

    set({ locale });
    applyDocumentLocale(locale);

    try {
      await invoke("set_setting", { key: DB_KEY, value: locale });
      // 同步更新原生托盘菜单语言
      invoke("update_tray_language", { locale }).catch(() => {});
      await emit(SYNC_EVENT, locale);
    } catch (error) {
      logError("Failed to save language setting:", error);
      set({ locale: previous });
      applyDocumentLocale(previous);
    }
  },
}));

let unlistenFn: (() => void) | null = null;

export async function initLocaleListener() {
  if (unlistenFn) return;
  try {
    unlistenFn = await listen<Locale>(SYNC_EVENT, (event) => {
      const locale = normalizeLocale(event.payload);
      useLocaleStore.setState({ locale });
      applyDocumentLocale(locale);
    });
  } catch {
    // non-Tauri environments
  }
}

const localeListeners = new Set<() => void>();

useLocaleStore.subscribe((state, prev) => {
  if (state.locale !== prev.locale) {
    for (const listener of localeListeners) {
      listener();
    }
  }
});

function subscribeLocale(listener: () => void) {
  localeListeners.add(listener);
  return () => localeListeners.delete(listener);
}

function getLocaleSnapshot() {
  return useLocaleStore.getState().locale;
}

export function getLocale(): Locale {
  return useLocaleStore.getState().locale;
}

export function t(key: string, params?: Record<string, string | number>) {
  return createTranslator(getLocale())(key, params);
}

export function useTranslation() {
  const locale = useSyncExternalStore(subscribeLocale, getLocaleSnapshot, () => DEFAULT_LOCALE);
  const translate = useMemo(() => createTranslator(locale), [locale]);
  const setLocale = useLocaleStore((s) => s.setLocale);
  return { t: translate, locale, setLocale };
}

export async function initLocale() {
  await initLocaleListener();
  await useLocaleStore.getState().init();
}
