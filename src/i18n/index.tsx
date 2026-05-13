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
const originalTextNodes = new WeakMap<Text, string>();
const originalAttributes = new WeakMap<Element, Map<string, string>>();
const TRANSLATABLE_ATTRIBUTES = ["placeholder", "aria-label", "title"] as const;

function normalizeLocale(value: string | null | undefined): Locale {
  return value === "en-US" ? "en-US" : DEFAULT_LOCALE;
}

function interpolate(template: string, values?: TranslationValues) {
  if (!values) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(values[name] ?? `{${name}}`));
}

const dynamicMessagePatterns = Object.entries(EN_MESSAGES)
  .filter(([key]) => key.includes("{") && key.includes("}") && !key.endsWith("__plural"))
  .map(([key, template]) => {
    const names: string[] = [];
    const pattern = key
      .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
      .replace(/\\\{(\w+)\\\}/g, (_, name: string) => {
        names.push(name);
        return "(.+?)";
      });
    return { regex: new RegExp(`^${pattern}$`), names, template };
  });

function translateDynamicPattern(key: string) {
  for (const { regex, names, template } of dynamicMessagePatterns) {
    const match = key.match(regex);
    if (!match) continue;
    const values = Object.fromEntries(names.map((name, index) => [name, match[index + 1]]));
    return interpolate(template, values);
  }
  return key;
}

export function translate(key: string, values?: TranslationValues, locale = currentLocale) {
  const template = locale === "en-US"
    ? EN_MESSAGES[key] ?? (values ? key : translateDynamicPattern(key))
    : key;
  if (
    locale === "en-US"
    && typeof values?.count === "number"
    && Number(values.count) !== 1
    && EN_MESSAGES[`${key}__plural`]
  ) {
    return interpolate(EN_MESSAGES[`${key}__plural`], values);
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

function translateDomSubtree(root: ParentNode, locale: Locale) {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let textNode = walker.nextNode() as Text | null;
  while (textNode) {
    const original = originalTextNodes.get(textNode) ?? textNode.nodeValue ?? "";
    originalTextNodes.set(textNode, original);
    const trimmed = original.trim();
    if (trimmed) {
      const translated = translate(trimmed, undefined, locale);
      textNode.nodeValue =
        locale === "en-US" && translated !== trimmed
          ? original.replace(trimmed, translated)
          : original;
    }
    textNode = walker.nextNode() as Text | null;
  }

  const elements = root instanceof Element ? [root, ...root.querySelectorAll("*")] : Array.from(root.childNodes).flatMap((node) =>
    node instanceof Element ? [node, ...node.querySelectorAll("*")] : [],
  );

  for (const element of elements) {
    const originalMap = originalAttributes.get(element) ?? new Map<string, string>();
    originalAttributes.set(element, originalMap);
    for (const attr of TRANSLATABLE_ATTRIBUTES) {
      const value = element.getAttribute(attr);
      if (!value) continue;
      if (!originalMap.has(attr)) {
        originalMap.set(attr, value);
      }
      const original = originalMap.get(attr) ?? value;
      const translated = translate(original, undefined, locale);
      if (locale === "en-US" && translated !== original) {
        element.setAttribute(attr, translated);
      } else if (locale !== "en-US" && element.getAttribute(attr) !== original) {
        element.setAttribute(attr, original);
      }
    }
  }
}

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(DEFAULT_LOCALE);

  useEffect(() => {
    applyLocale(locale);
  }, [locale]);

  useEffect(() => {
    translateDomSubtree(document.body, locale);
    let frame = 0;
    const observer = new MutationObserver((mutations) => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        for (const mutation of mutations) {
          if (mutation.type === "characterData" && mutation.target.parentNode) {
            translateDomSubtree(mutation.target.parentNode, locale);
            continue;
          }
          if (mutation.target instanceof Element) {
            translateDomSubtree(mutation.target, locale);
          }
          mutation.addedNodes.forEach((node) => {
            if (node instanceof Element || node instanceof DocumentFragment) {
              translateDomSubtree(node, locale);
            }
          });
        }
      });
    });
    observer.observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: [...TRANSLATABLE_ATTRIBUTES],
    });
    return () => {
      observer.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
    };
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
