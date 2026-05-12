import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { logError } from "@/lib/logger";
import {
  DEFAULT_LOCALE,
  EN_MESSAGES,
  LANGUAGE_SETTING_KEY,
  LANGUAGE_OPTIONS,
  type Locale,
} from "./messages";

type TranslationValues = Record<string, string | number>;

interface I18nContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => Promise<void>;
  t: (key: string, values?: TranslationValues) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

let currentLocale: Locale = DEFAULT_LOCALE;

function normalizeLocale(value: string | null | undefined): Locale {
  return value === "en-US" ? "en-US" : DEFAULT_LOCALE;
}

function interpolate(template: string, values?: TranslationValues) {
  if (!values) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(values[name] ?? `{${name}}`));
}

export function translate(key: string, values?: TranslationValues, locale = currentLocale) {
  const template = locale === "en-US" ? EN_MESSAGES[key] ?? key : key;
  if (locale === "en-US" && key === "{count} 天前" && Number(values?.count) !== 1) {
    return interpolate(EN_MESSAGES["{count} 天前__plural"] ?? template, values);
  }
  return interpolate(template, values);
}

export function getCurrentLocale() {
  return currentLocale;
}

function applyLocale(locale: Locale) {
  currentLocale = locale;
  document.documentElement.lang = locale;
}

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(DEFAULT_LOCALE);

  useEffect(() => {
    applyLocale(locale);
  }, [locale]);

  useEffect(() => {
    let mounted = true;
    invoke<string | null>("get_setting", { key: LANGUAGE_SETTING_KEY })
      .then((value) => {
        if (!mounted) return;
        setLocaleState(normalizeLocale(value));
      })
      .catch((error) => {
        logError("Failed to load interface language:", error);
      });

    const unlisten = listen<string>("interface-language-changed", (event) => {
      setLocaleState(normalizeLocale(event.payload));
    });

    return () => {
      mounted = false;
      void unlisten.then((fn) => fn());
    };
  }, []);

  const setLocale = useCallback(async (nextLocale: Locale) => {
    setLocaleState(nextLocale);
    try {
      await invoke("set_setting", {
        key: LANGUAGE_SETTING_KEY,
        value: nextLocale,
      });
    } catch (error) {
      logError("Failed to save interface language:", error);
      setLocaleState(locale);
    }
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => ({
    locale,
    setLocale,
    t: (key, values) => translate(key, values, locale),
  }), [locale, setLocale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used within I18nProvider");
  }
  return context;
}

export { LANGUAGE_OPTIONS, type Locale };
