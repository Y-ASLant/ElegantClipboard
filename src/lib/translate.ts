import { invoke } from "@tauri-apps/api/core";
import { t } from "@/i18n";
import { logError } from "@/lib/logger";
import { useTranslateSettings, type TranslateProvider } from "@/stores/translate-settings";

export function getLanguages() {
  return [
    { value: "zh", label: t("translate.lang.zh") },
    { value: "en", label: t("translate.lang.en") },
    { value: "ja", label: t("translate.lang.ja") },
    { value: "ko", label: t("translate.lang.ko") },
    { value: "th", label: t("translate.lang.th") },
    { value: "fr", label: t("translate.lang.fr") },
    { value: "de", label: t("translate.lang.de") },
    { value: "ru", label: t("translate.lang.ru") },
    { value: "vi", label: t("translate.lang.vi") },
    { value: "es", label: t("translate.lang.es") },
    { value: "pt", label: t("translate.lang.pt") },
    { value: "ar", label: t("translate.lang.ar") },
    { value: "it", label: t("translate.lang.it") },
  ];
}

/** @deprecated use getLanguages() */
export const LANGUAGES = getLanguages();

function isChinese(text: string): boolean {
  const sample = text.slice(0, 200);
  let cjkCount = 0;
  let kanaCount = 0;
  let totalLetters = 0;
  for (const ch of sample) {
    const code = ch.codePointAt(0) ?? 0;
    if (
      (code >= 0x4e00 && code <= 0x9fff) ||
      (code >= 0x3400 && code <= 0x4dbf) ||
      (code >= 0x20000 && code <= 0x2a6df)
    ) {
      cjkCount++;
    }
    // 日文假名（平假名 + 片假名）→ 非中文
    if (
      (code >= 0x3040 && code <= 0x309f) ||
      (code >= 0x30a0 && code <= 0x30ff)
    ) {
      kanaCount++;
    }
    if (/\p{L}/u.test(ch)) totalLetters++;
  }
  // 含日文假名则判定为日文，非中文
  if (kanaCount > 0) return false;
  return totalLetters > 0 && cjkCount / totalLetters > 0.3;
}

function resolveLanguages(text: string): { from: string; to: string } {
  const settings = useTranslateSettings.getState();
  if (settings.languageMode === "manual" && settings.sourceLanguage && settings.targetLanguage) {
    return { from: settings.sourceLanguage, to: settings.targetLanguage };
  }
  if (isChinese(text)) {
    return { from: "zh", to: "en" };
  }
  return { from: "auto", to: "zh" };
}

/** 解析后端结构化错误码，返回本地化错误消息 */
function localizeTranslateError(error: unknown): string {
  if (typeof error !== "string" || !error.startsWith("TRANSLATE:")) {
    return t("operationFeedback.unknownReason");
  }
  const code = error.split(":", 2)[1];
  const i18nKey = `translate.errors.${code}`;
  // Provider details can contain response bodies, URLs or credentials.
  const localized = t(i18nKey, { detail: t("operationFeedback.unknownReason") });
  return localized !== i18nKey ? localized : t("operationFeedback.unknownReason");
}

export async function translateText(text: string): Promise<string> {
  const settings = useTranslateSettings.getState();
  if (!settings.enabled) throw new Error(t("translate.errors.FEATURE_DISABLED"));

  const { from, to } = resolveLanguages(text);

  try {
    return await invoke<string>("translate_text", {
      text,
      from,
      to,
      provider: settings.provider,
      proxyMode: settings.proxyMode,
      proxyUrl: settings.proxyUrl,
      deeplxEndpoint: settings.deeplxEndpoint || null,
      googleApiKey: settings.googleApiKey || null,
      baiduAppId: settings.baiduAppId || null,
      baiduSecretKey: settings.baiduSecretKey || null,
      openaiEndpoint: settings.openaiEndpoint || null,
      openaiApiKey: settings.openaiApiKey || null,
      openaiModel: settings.openaiModel || null,
    });
  } catch (error) {
    logError("翻译失败:", error);
    throw new Error(localizeTranslateError(error));
  }
}

export function getProviderOptions(): { value: TranslateProvider; label: string; needsConfig: boolean }[] {
  return [
    { value: "microsoft", label: t("translate.provider.microsoft"), needsConfig: false },
    { value: "google_free", label: t("translate.provider.googleFree"), needsConfig: false },
    { value: "google_api", label: t("translate.provider.googleApi"), needsConfig: true },
    { value: "baidu", label: t("translate.provider.baidu"), needsConfig: true },
    { value: "deeplx", label: "DeepLX", needsConfig: true },
    { value: "openai", label: "OpenAI / AI", needsConfig: true },
  ];
}

/** @deprecated use getProviderOptions() */
export const PROVIDER_OPTIONS = getProviderOptions();
