import { en } from "../messages/en";
import { zhCN } from "../messages/zh-CN";
import { zhTW } from "../messages/zh-TW";
import type { Locale, TranslationTree } from "../types";

export const DEFAULT_LOCALE: Locale = "zh-CN";

export const LOCALE_OPTIONS: {
  value: Locale;
  labelKey: "language.zhCN" | "language.en" | "language.zhTW";
}[] = [
  { value: "zh-CN", labelKey: "language.zhCN" },
  { value: "en", labelKey: "language.en" },
  { value: "zh-TW", labelKey: "language.zhTW" },
];

const LOCALES: Record<Locale, TranslationTree> = {
  "zh-CN": zhCN as TranslationTree,
  en: en as TranslationTree,
  "zh-TW": zhTW as TranslationTree,
};

function resolvePath(tree: TranslationTree, path: string): string | undefined {
  let current: unknown = tree;
  for (const part of path.split(".")) {
    if (!current || typeof current !== "object" || !(part in current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }
  return typeof current === "string" ? current : undefined;
}

const warnedKeys = new Set<string>();
const PLACEHOLDER_PATTERN = /\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}/g;

function warnMissingKey(key: string, locale: Locale) {
  if (import.meta.env.PROD) return;
  if (warnedKeys.has(key)) return;
  warnedKeys.add(key);
  console.warn(`[i18n] Missing translation key "${key}" in locale "${locale}" (and default locale)`);
}

export function createTranslator(locale: Locale) {
  const tree = LOCALES[locale] ?? LOCALES[DEFAULT_LOCALE];
  return (key: string, params?: Record<string, string | number>) => {
    const resolved =
      resolvePath(tree, key) ?? resolvePath(LOCALES[DEFAULT_LOCALE], key);
    if (resolved === undefined) {
      warnMissingKey(key, locale);
      return key;
    }
    let text = resolved;
    if (params) {
      for (const [name, value] of Object.entries(params)) {
        text = text.split(`{{${name}}}`).join(String(value));
      }
    }
    // 清空未匹配占位符，避免 UI 出现 {{xxx}} 字面量
    text = text.replace(PLACEHOLDER_PATTERN, "");
    return text;
  };
}

export function normalizeLocale(raw: string | null | undefined): Locale {
  if (raw === "en" || raw === "zh-TW" || raw === "zh-CN") {
    return raw;
  }
  return DEFAULT_LOCALE;
}

export function applyDocumentLocale(locale: Locale) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale;
  }
}
